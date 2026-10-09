//! The G50 helper (RPT-048, réel regime): the monthly figures with the boxes of the form from the
//! data pack — CA HT, exempt and taxable turnover and TVA per block of 9 % and 19 % (boxes
//! 1–12), deductible TVA (G9 annex, CTCA art. 29-30), TVA due or credit carried forward, and the
//! stamp duty "sur état" (§4, Code du timbre art. 4 and 19). IRG salaires is typed on the
//! screen. Any other rate a future pack adds is listed apart rather than guessed into a box.

use crate::datapack::Pack;
use crate::error::AppError;
use crate::money::{Money, Rate};
use crate::reports::{Grouping, MAX_ROW_LIMIT, ReportKind, ReportRequest, run_with};
use crate::tax::tva::RateTotal;
use chrono::{Datelike, NaiveDate};
use rusqlite::Connection;
use serde::Serialize;
use specta::Type;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct G50Line {
    pub key: String,
    /// The box on the form (`""` for the lines the form has no box for).
    pub r#box: String,
    pub amount: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct G50 {
    pub from: String,
    pub to: String,
    pub form: String,
    pub lines: Vec<G50Line>,
    pub other_rates: Vec<RateTotal>,
    pub tva_collected: Money,
    pub tva_deductible: Money,
    pub tva_due: Money,
    pub tva_credit: Money,
    pub stamp_duty: Money,
}

/// `YYYY-MM` → its first and last day.
pub fn month_range(month: &str) -> Result<(String, String), AppError> {
    let invalid = || AppError::validation("month", "date_invalid");
    let first =
        NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").map_err(|_| invalid())?;
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
    }
    .ok_or_else(invalid)?;
    let last = next.pred_opt().ok_or_else(invalid)?;
    Ok((
        first.format("%Y-%m-%d").to_string(),
        last.format("%Y-%m-%d").to_string(),
    ))
}

/// Runs the tax report `kind` over the range with every row (no filter).
fn full(
    conn: &Connection,
    pack: &Pack,
    kind: ReportKind,
    from: &str,
    to: &str,
) -> Result<crate::reports::Report, AppError> {
    let mut req = ReportRequest::new(kind, from, to);
    req.group = Some(Grouping::Year);
    req.limit = Some(MAX_ROW_LIMIT);
    run_with(conn, &req, pack)
}

/// The G50 of the range (a month: [`month_range`]), from the same reports the screens show: the
/// TVA per rate, the deductible TVA and the timbre statement.
pub fn g50(conn: &Connection, pack: &Pack, from: &str, to: &str) -> Result<G50, AppError> {
    let vat = full(conn, pack, ReportKind::VatByRate, from, to)?;
    let (rate_col, base_col, tax_col) = (
        vat.table.column("rate").unwrap_or(1),
        vat.table.column("baseHt").unwrap_or(2),
        vat.table.column("tax").unwrap_or(3),
    );
    let cell = |row: &[crate::table::Cell], i: usize| row.get(i).and_then(|c| c.as_i64());
    let mut by_rate: BTreeMap<i64, (i64, i64)> = BTreeMap::new();
    for row in &vat.table.rows {
        let slot = by_rate.entry(cell(row, rate_col).unwrap_or(0)).or_default();
        slot.0 = slot.0.saturating_add(cell(row, base_col).unwrap_or(0));
        slot.1 = slot.1.saturating_add(cell(row, tax_col).unwrap_or(0));
    }
    let (base9, tva9) = by_rate.get(&900).copied().unwrap_or_default();
    let (base19, tva19) = by_rate.get(&1_900).copied().unwrap_or_default();
    let exempt = by_rate.get(&0).map_or(0, |v| v.0);
    // TAX-009: exempt turnover goes into the block the pack names (to confirm with the DGI).
    let (exempt9, exempt19) = if pack.fiscal.g50.exempt_block == "9" {
        (exempt, 0)
    } else {
        (0, exempt)
    };
    let other_rates = by_rate
        .iter()
        .filter(|(r, _)| ![0, 900, 1_900].contains(*r))
        .map(|(r, (b, t))| RateTotal {
            rate_bp: Rate(i32::try_from(*r).unwrap_or(0)),
            base_ht: Money(*b),
            tax: Money(*t),
            total_ttc: Money(b.saturating_add(*t)),
        })
        .collect();
    let tva_deductible = full(conn, pack, ReportKind::VatDeductible, from, to)?
        .table
        .money_total("deductibleTax");
    let stamp_duty = full(conn, pack, ReportKind::StampDutyStatement, from, to)?
        .table
        .money_total("dutyTotal");
    let tva_collected = Money(tva9.saturating_add(tva19));
    let net = tva_collected.0.saturating_sub(tva_deductible.0);
    let (tva_due, tva_credit) = (Money(net.max(0)), Money(net.saturating_neg().max(0)));
    let boxes = &pack.fiscal.g50.boxes;
    let line = |key: &str, amount: i64| G50Line {
        key: key.to_owned(),
        r#box: boxes.get(key).cloned().unwrap_or_default(),
        amount: Money(amount),
    };
    let lines = vec![
        line("caHt9", base9.saturating_add(exempt9)),
        line("exempt9", exempt9),
        line("taxable9", base9),
        line("tva9", tva9),
        line("caHt19", base19.saturating_add(exempt19)),
        line("exempt19", exempt19),
        line("taxable19", base19),
        line("tva19", tva19),
        line(
            "caHtTotal",
            base9.saturating_add(base19).saturating_add(exempt),
        ),
        line("exemptTotal", exempt),
        line("taxableTotal", base9.saturating_add(base19)),
        line("tvaTotal", tva_collected.0),
        line("tvaDeductible", tva_deductible.0),
        line("tvaDue", tva_due.0),
        line("tvaCredit", tva_credit.0),
        line("stampDuty", stamp_duty.0),
    ];
    Ok(G50 {
        from: from.to_owned(),
        to: to.to_owned(),
        form: pack.fiscal.g50.form.clone(),
        lines,
        other_rates,
        tva_collected,
        tva_deductible,
        tva_due,
        tva_credit,
        stamp_duty,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Qty;
    use crate::purchases::invoices::{InvoiceInput, PaymentNow, record_invoice};
    use crate::purchases::lines::PurchaseLineInput;
    use crate::reports::{ReportKind, ReportRequest, run_with};
    use crate::table::Cell;
    use crate::tax::tva::Discount;
    use crate::testkit::shop;

    /// M2 has no `PurchaseLineInput::new`: the line is built literally.
    fn line(product: &str, qty: i64, unit_cost: i64) -> PurchaseLineInput {
        PurchaseLineInput {
            product_id: product.to_owned(),
            qty: Qty(qty),
            unit_cost: Money(unit_cost),
            pack_qty: None,
            tax_rate_id: None,
            discount: Discount::None,
            origin_line_id: None,
            lot_number: None,
            expiry_date: None,
            serials: Vec::new(),
            note: None,
        }
    }

    fn purchase(
        shop: &crate::testkit::Shop,
        supplier: &str,
        product: &str,
        unit_cost: i64,
        qty: i64,
        method: crate::payments::PaymentMethod,
    ) {
        let ctx = shop.owner_till();
        let input = InvoiceInput {
            supplier_id: supplier.to_owned(),
            supplier_number: format!("F-{unit_cost}"),
            receipt_ids: Vec::new(),
            lines: vec![line(product, qty, unit_cost)],
            note: None,
            payment: Some(PaymentNow {
                amount: Money(unit_cost * qty / 1_000 * 119 / 100),
                method,
                reference: Some("REF".into()),
                cheque_bank: None,
                cheque_date: None,
                from_till: false,
            }),
        };
        shop.db
            .write(move |tx| record_invoice(tx, &ctx, input).map(drop))
            .unwrap();
    }

    #[test]
    fn a_cash_paid_invoice_above_the_limit_is_not_deductible() {
        let shop = shop();
        let supplier = shop.supplier("Cimenterie Chlef");
        let cement = shop.product("Ciment 25 kg", Money(119_000), &shop.tva19);
        // 900 000.00 HT, 1 071 000.00 TTC, paid by transfer.
        purchase(
            &shop,
            &supplier.id,
            &cement.id,
            90_000,
            1_000_000,
            crate::payments::PaymentMethod::Transfer,
        );
        // 1 082 900.00 TTC paid in cash.
        purchase(
            &shop,
            &supplier.id,
            &cement.id,
            91_000,
            1_000_000,
            crate::payments::PaymentMethod::Cash,
        );
        let req = ReportRequest::new(ReportKind::VatDeductible, "2026-09-01", "2026-09-30");
        let pack = shop.pack.clone();
        let r = shop.db.read(move |c| run_with(c, &req, &pack)).unwrap();
        let deductible = r.table.column("deductibleTax").unwrap();
        let reason = r.table.column("reason").unwrap();
        assert_eq!(r.table.rows[0][deductible], Cell::money(Money(17_100_000)));
        assert_eq!(r.table.rows[1][deductible], Cell::money(Money::ZERO));
        assert_eq!(r.table.rows[1][reason], Cell::text("cash_over_limit"));
        let pack = shop.pack.clone();
        let g = shop
            .db
            .read(move |c| g50(c, &pack, "2026-09-01", "2026-09-30"))
            .unwrap();
        assert_eq!(g.tva_deductible, Money(17_100_000));
        assert_eq!(
            g.tva_credit,
            Money(17_100_000),
            "no sales: a TVA credit carried forward"
        );
    }

    #[test]
    fn a_supplier_return_takes_its_tva_back_from_the_deduction() {
        use crate::purchases::returns::{
            SupplierReturnInput, SupplierReturnLine, return_to_supplier,
        };
        let shop = shop();
        let supplier = shop.supplier("Cimenterie Chlef");
        let cement = shop.product("Ciment 25 kg", Money(119_000), &shop.tva19);
        purchase(
            &shop,
            &supplier.id,
            &cement.id,
            90_000,
            1_000_000,
            crate::payments::PaymentMethod::Transfer,
        );
        // 100 bags back at 900.00 HT: 90 000.00 HT, 17 100.00 of TVA.
        let (ctx, input) = (
            shop.owner_till(),
            SupplierReturnInput {
                supplier_id: supplier.id.clone(),
                location_id: None,
                lines: vec![SupplierReturnLine {
                    product_id: cement.id.clone(),
                    qty: Qty(100_000),
                    unit_cost: Some(Money(90_000)),
                    lot_number: None,
                    serials: Vec::new(),
                    note: None,
                }],
                credit_supplier: true,
                note: None,
            },
        );
        shop.db
            .write(move |tx| return_to_supplier(tx, &ctx, input).map(drop))
            .unwrap();
        let req = ReportRequest::new(ReportKind::VatDeductible, "2026-09-01", "2026-09-30");
        let pack = shop.pack.clone();
        let r = shop.db.read(move |c| run_with(c, &req, &pack)).unwrap();
        let (number, deductible) = (
            r.table.column("number").unwrap(),
            r.table.column("deductibleTax").unwrap(),
        );
        let back = r
            .table
            .rows
            .iter()
            .find(|row| row[number].as_str().is_some_and(|n| n.starts_with("BR")))
            .unwrap();
        assert_eq!(back[deductible], Cell::money(Money(-1_710_000)));
        let pack = shop.pack.clone();
        let g = shop
            .db
            .read(move |c| g50(c, &pack, "2026-09-01", "2026-09-30"))
            .unwrap();
        assert_eq!(g.tva_deductible, Money(17_100_000 - 1_710_000));
    }

    #[test]
    fn a_month_is_its_first_and_last_day() {
        assert_eq!(
            month_range("2026-02").unwrap(),
            ("2026-02-01".into(), "2026-02-28".into())
        );
        assert_eq!(
            month_range("2028-02").unwrap(),
            ("2028-02-01".into(), "2028-02-29".into())
        );
        assert_eq!(month_range("2026-13").unwrap_err().code(), "date_invalid");
    }
}
