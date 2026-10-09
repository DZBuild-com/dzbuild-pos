//! Versement receipts (CRD-004, TAX-007) and account statements (RPT-037) for the receipt
//! printer, as `pos_hw::layout` rows: the shop header of the ticket, pairs, bold totals; Arabic
//! is rastered by `pos_hw::render` like any RTL layout. They are laid out here because M2 left
//! `pos-hw` to the hardware milestone (it only gained `Registry::print_layout`); they can move next to
//! `receipt_layout` as they are. The M8 pack slips (intake, deposit, vouchers, kitchen) are
//! `pos_hw::slips`.

use pos_core::lang::Lang;
use pos_core::money::Money;
use pos_docs::receipt::{ReceiptHeader, label};
use pos_docs::settlement::{SettlementReceipt, StatementPrint};
use pos_hw::fmt::{amount, currency};
use pos_hw::layout::{Align, Layout, Row};

fn text(text: impl Into<String>, align: Align) -> Row {
    Row::Text {
        text: text.into(),
        align,
        large: false,
        bold: false,
    }
}

fn strong(text: impl Into<String>, align: Align) -> Row {
    Row::Text {
        text: text.into(),
        align,
        large: false,
        bold: true,
    }
}

fn pair(start: impl Into<String>, end: impl Into<String>) -> Row {
    Row::Pair {
        start: start.into(),
        end: end.into(),
        large: false,
        bold: false,
    }
}

fn strong_pair(start: impl Into<String>, end: impl Into<String>) -> Row {
    Row::Pair {
        start: start.into(),
        end: end.into(),
        large: false,
        bold: true,
    }
}

/// The figure the customer looks for, with the currency, in large print.
fn total(start: impl Into<String>, value: Money, lang: Lang) -> Row {
    Row::Pair {
        start: start.into(),
        end: format!("{} {}", amount(value, lang), currency(lang)),
        large: true,
        bold: true,
    }
}

/// The shop's identity as the ticket prints it (DOC-005 mentions).
fn header(h: &ReceiptHeader, lang: Lang) -> Vec<Row> {
    let mut rows = vec![Row::Text {
        text: h.name.clone(),
        align: Align::Center,
        large: true,
        bold: true,
    }];
    for value in [&h.legal_form, &h.address].into_iter().flatten() {
        rows.push(text(value.clone(), Align::Center));
    }
    if let Some(phone) = &h.phone {
        rows.push(text(
            format!("{} {phone}", label(lang, "receipt.phone")),
            Align::Center,
        ));
    }
    for (value, key) in [
        (&h.rc, "receipt.rc"),
        (&h.nif, "receipt.nif"),
        (&h.nis, "receipt.nis"),
        (&h.ai, "receipt.ai"),
    ] {
        if let Some(value) = value {
            rows.push(text(format!("{} {value}", label(lang, key)), Align::Center));
        }
    }
    if let Some(header) = &h.header_text {
        rows.push(text(header.clone(), Align::Center));
    }
    // Finding 39: a slip of the demo shop can never pass for a real one.
    if h.demo {
        rows.push(Row::Text {
            text: label(lang, "receipt.demo"),
            align: Align::Center,
            large: true,
            bold: true,
        });
    }
    rows.push(Row::Rule);
    rows
}

/// The reçu de versement: both balances, the duty charged on the cash (never recomputed here),
/// what was handed over, the debts settled. The customer's phone is not printed (loi 18-07).
pub fn settlement_layout(r: &SettlementReceipt) -> Layout {
    let l = &r.labels;
    let a = |m: Money| amount(m, r.lang);
    let mut rows = header(&r.header, r.lang);
    rows.push(strong(l.title.clone(), Align::Center));
    if r.duplicate {
        rows.push(Row::Text {
            text: l.duplicate.clone(),
            align: Align::Center,
            large: true,
            bold: true,
        });
    }
    rows.push(text(format!("{} {}", l.number, r.number), Align::Start));
    rows.push(pair(
        format!("{} {}", l.date, r.local_time),
        format!("{} {}", l.terminal, r.terminal_code),
    ));
    rows.push(text(format!("{} {}", l.cashier, r.cashier), Align::Start));
    rows.push(Row::Rule);
    rows.push(pair(l.customer.clone(), r.customer_name.clone()));
    rows.push(pair(l.previous_balance.clone(), a(r.balance_before)));
    rows.push(pair(l.amount.clone(), a(r.amount)));
    if r.stamp_duty != Money::ZERO {
        rows.push(pair(l.stamp_duty.clone(), a(r.stamp_duty)));
    }
    rows.push(total(l.received.clone(), r.total_received, r.lang));
    rows.push(pair(l.method.clone(), r.method_label.clone()));
    if let Some(reference) = &r.reference {
        rows.push(text(format!("  {reference}"), Align::Start));
    }
    if r.stamp_absorbed != Money::ZERO {
        rows.push(text(
            format!("{} : {}", l.stamp_absorbed, a(r.stamp_absorbed)),
            Align::Start,
        ));
    }
    rows.push(Row::Rule);
    match r.balance_after.checked_neg() {
        // CRD-007: paid above the debt, the rest is an advance in the customer's favour.
        Some(advance) if r.balance_after.is_negative() => {
            rows.push(strong_pair(l.advance.clone(), a(advance)));
        }
        _ => rows.push(strong_pair(l.new_balance.clone(), a(r.balance_after))),
    }
    if !r.settled.is_empty() {
        rows.push(Row::Rule);
        rows.push(strong(l.settled.clone(), Align::Start));
        for settled in &r.settled {
            rows.push(pair(settled.number.clone(), a(settled.amount)));
        }
    }
    rows.push(Row::Rule);
    rows.push(text(
        format!("{} : {}", l.in_letters, r.total_in_letters),
        Align::Start,
    ));
    rows.push(strong(l.thanks.clone(), Align::Center));
    rows.push(Row::Feed { lines: 1 });
    Layout { rtl: r.rtl, rows }
}

/// The account statement: the opening balance, each entry with its document number and the
/// running balance after it (`= …`), the closing balance.
pub fn statement_layout(p: &StatementPrint) -> Layout {
    let a = |m: Money| amount(m, p.lang);
    let mut rows = header(&p.header, p.lang);
    rows.push(strong(p.title.clone(), Align::Center));
    rows.push(pair(p.party_name.clone(), p.party_code.clone()));
    rows.push(text(p.period.clone(), Align::Center));
    rows.push(Row::Rule);
    rows.push(strong_pair(p.opening_label.clone(), a(p.opening)));
    for line in &p.lines {
        rows.push(pair(
            format!("{} {}", line.date, line.label),
            a(line.amount),
        ));
        rows.push(pair(
            format!("  {}", line.number.as_deref().unwrap_or_default()),
            format!("= {}", a(line.balance)),
        ));
    }
    rows.push(Row::Rule);
    rows.push(total(p.closing_label.clone(), p.closing, p.lang));
    rows.push(text(p.printed_at.clone(), Align::Center));
    rows.push(Row::Feed { lines: 1 });
    Layout { rtl: p.rtl, rows }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_core::company::{Company, PriceMode, TaxRegime};
    use pos_core::ledger::{EntryKind, LedgerEntry, Statement};
    use pos_core::payments::PaymentMethod;
    use pos_docs::settlement::{
        SettledLine, SettlementSource, build_settlement, build_statement_print,
    };
    use pos_hw::layout::Row;
    use pos_hw::profile::{PaperWidth, Preset, Profile};
    use pos_hw::render::uses_raster;

    fn company() -> Company {
        Company {
            name_ar: "مغازة الأمل".into(),
            name_fr: "Supérette El Amel".into(),
            name_en: String::new(),
            legal_form: None,
            capital: None,
            address: Some("12 rue Larbi Ben M'hidi, Alger".into()),
            wilaya: Some("16".into()),
            phone: Some("023 45 67 89".into()),
            email: None,
            rc: None,
            nif: Some("000116001234567".into()),
            nis: None,
            ai: None,
            rib: None,
            tax_regime: TaxRegime::Ifu,
            price_mode: PriceMode::Ttc,
            receipt_header_ar: None,
            receipt_header_fr: None,
            receipt_header_en: None,
            receipt_footer_ar: None,
            receipt_footer_fr: None,
            receipt_footer_en: None,
            default_doc_lang: Lang::Fr,
            has_logo: false,
            updated_at: 0,
            demo: false,
        }
    }

    fn entry(kind: EntryKind, amount: i64, balance_after: i64) -> LedgerEntry {
        LedgerEntry {
            id: "e1".into(),
            party_id: "c1".into(),
            kind,
            amount: Money(amount),
            balance_after: Money(balance_after),
            method: Some(PaymentMethod::Cash),
            reference: None,
            cheque_bank: None,
            cheque_date: None,
            stamp_duty: Money(2_000),
            stamp_charged: Money(2_000),
            receipt_number: Some("RV2026-000001".into()),
            ref_type: None,
            ref_id: None,
            ref_number: None,
            cash_session_id: Some("s1".into()),
            business_date: "2026-09-28".into(),
            note: None,
            created_at: 1_790_593_200_000,
            user_id: Some("u1".into()),
            terminal_id: Some("t1".into()),
            open: Money::ZERO,
        }
    }

    /// Karim pays 2 000 DA in cash on a debt of 3 000 DA, 20 DA of duty charged; 1 500 DA of it
    /// settles a ticket.
    fn versement(lang: Lang) -> SettlementReceipt {
        let company = company();
        let entry = entry(EntryKind::Payment, -200_000, 100_000);
        let settled = [SettledLine {
            number: "TK2026-C1-000012".into(),
            amount: Money(150_000),
        }];
        build_settlement(&SettlementSource {
            company: &company,
            customer_name: "Karim Benali",
            customer_phone: Some("0550123456"),
            entry: &entry,
            settled: &settled,
            cashier_name: "Yacine",
            terminal_code: "C1",
            lang,
            duplicate: false,
        })
        .unwrap()
    }

    fn statement(lang: Lang) -> StatementPrint {
        let mut sale = entry(EntryKind::SaleCredit, 200_000, 700_000);
        sale.ref_number = Some("TK2026-C1-000012".into());
        sale.receipt_number = None;
        sale.business_date = "2026-09-12".into();
        let payment = entry(EntryKind::Payment, -200_000, 500_000);
        let statement = Statement {
            party_id: "c1".into(),
            from: "2026-09-01".into(),
            to: "2026-09-30".into(),
            opening: Money(500_000),
            entries: vec![sale, payment],
            debits: Money(200_000),
            credits: Money(200_000),
            closing: Money(500_000),
        };
        build_statement_print(
            &company(),
            "Karim Benali",
            "C00001",
            &statement,
            lang,
            1_790_593_200_000,
        )
    }

    /// One line per row: `*` bold, `**` large and bold, a pair as its two sides.
    fn text(layout: &Layout) -> String {
        let mark = |large: bool, bold: bool| match (large, bold) {
            (true, _) => "** ",
            (false, true) => "* ",
            (false, false) => "",
        };
        layout
            .rows
            .iter()
            .map(|row| match row {
                Row::Text {
                    text, large, bold, ..
                } => format!("{}{text}", mark(*large, *bold)),
                Row::Pair {
                    start,
                    end,
                    large,
                    bold,
                } => format!("{}{start} {end}", mark(*large, *bold)),
                Row::Rule => "--------".into(),
                Row::Feed { lines } => format!("(feed {lines})"),
                Row::Barcode { data } => format!("(barcode {data})"),
                Row::Logo => "(logo)".into(),
                Row::Qr { size, .. } => format!("(qr {size})"),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn profile() -> Profile {
        Profile::preset(Preset::Generic, PaperWidth::Mm80)
    }

    #[test]
    fn a_versement_receipt_shows_the_duty_and_both_balances() {
        let fr = settlement_layout(&versement(Lang::Fr));
        let printed = text(&fr);
        for expected in [
            "Reçu de versement",
            "RV2026-000001",
            "Droit de timbre 20,00",
            "Nouveau solde 1 000,00",
            "TK2026-C1-000012 1 500,00",
        ] {
            assert!(printed.contains(expected), "{expected} in\n{printed}");
        }
        assert!(!uses_raster(&fr, &profile()), "French prints in text mode");
        insta::assert_snapshot!("versement_fr", printed);
        let ar = settlement_layout(&versement(Lang::Ar));
        assert!(uses_raster(&ar, &profile()), "Arabic is rastered");
        insta::assert_snapshot!("versement_ar", text(&ar));
    }

    #[test]
    fn an_advance_is_printed_as_the_customers_credit() {
        let mut r = versement(Lang::Fr);
        r.balance_after = Money(-50_000);
        let printed = text(&settlement_layout(&r));
        assert!(
            printed.contains("Avance en votre faveur 500,00"),
            "{printed}"
        );
        assert!(!printed.contains("Nouveau solde"));
    }

    #[test]
    fn a_statement_lists_every_entry_with_the_running_balance() {
        let fr = statement_layout(&statement(Lang::Fr));
        let printed = text(&fr);
        for expected in [
            "Relevé de compte",
            "Karim Benali C00001",
            "Solde au début 5 000,00",
            "12/09/2026 Vente à crédit 2 000,00",
            "TK2026-C1-000012 = 7 000,00",
        ] {
            assert!(printed.contains(expected), "{expected} in\n{printed}");
        }
        insta::assert_snapshot!("statement_fr", printed);
        let ar = statement_layout(&statement(Lang::Ar));
        assert!(uses_raster(&ar, &profile()), "Arabic is rastered");
        insta::assert_snapshot!("statement_ar", text(&ar));
    }
}
