//! The versement receipt (reçu de versement, CRD-004, TAX-007) and the account statement for the
//! receipt printer (RPT-037): language-independent models that `pos-hw` lays out, like the
//! ticket's `Receipt`.

use crate::letters::amount_in_letters;
use crate::locales::text;
use crate::receipt::{ReceiptHeader, method_label, receipt_header};
use pos_core::company::Company;
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::ledger::{EntryKind, LedgerEntry, Statement};
use pos_core::money::Money;
use pos_core::payments::PaymentMethod;
use pos_core::time::algiers_datetime;
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SettlementLabels {
    pub title: String,
    pub duplicate: String,
    pub number: String,
    pub date: String,
    pub cashier: String,
    pub terminal: String,
    pub customer: String,
    pub previous_balance: String,
    pub amount: String,
    pub stamp_duty: String,
    pub stamp_absorbed: String,
    pub received: String,
    pub method: String,
    pub new_balance: String,
    pub advance: String,
    pub settled: String,
    pub in_letters: String,
    pub thanks: String,
}

/// A debt the versement settled: its document or receipt number and the part paid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SettledLine {
    pub number: String,
    pub amount: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SettlementReceipt {
    pub lang: Lang,
    pub rtl: bool,
    pub number: String,
    /// `dd/mm/yyyy HH:MM`, Algiers time of the versement.
    pub local_time: String,
    pub cashier: String,
    pub terminal_code: String,
    pub header: ReceiptHeader,
    pub customer_name: String,
    pub customer_phone: Option<String>,
    pub balance_before: Money,
    pub amount: Money,
    /// Duty charged to the customer.
    pub stamp_duty: Money,
    /// Duty the shop pays itself.
    pub stamp_absorbed: Money,
    /// What the customer handed over: the versement and the duty charged.
    pub total_received: Money,
    pub method: PaymentMethod,
    pub method_label: String,
    pub reference: Option<String>,
    /// Negative: an advance in the customer's favour (CRD-007).
    pub balance_after: Money,
    pub settled: Vec<SettledLine>,
    pub total_in_letters: String,
    pub duplicate: bool,
    pub labels: SettlementLabels,
}

/// Everything a versement receipt needs besides the ledger entry's figures.
#[derive(Debug, Clone, Copy)]
pub struct SettlementSource<'a> {
    pub company: &'a Company,
    pub customer_name: &'a str,
    pub customer_phone: Option<&'a str>,
    pub entry: &'a LedgerEntry,
    pub settled: &'a [SettledLine],
    pub cashier_name: &'a str,
    pub terminal_code: &'a str,
    pub lang: Lang,
    pub duplicate: bool,
}

/// A document word in `lang`, else in French, else the key (the parity tests keep them full).
fn word(lang: Lang, key: &str) -> String {
    text(lang.as_str(), key)
        .or_else(|| text("fr", key))
        .unwrap_or_else(|| key.to_owned())
}

fn labels(lang: Lang) -> SettlementLabels {
    let w = |k: &str| word(lang, &format!("settlement.{k}"));
    SettlementLabels {
        title: w("title"),
        duplicate: w("duplicate"),
        number: w("number"),
        date: w("date"),
        cashier: w("cashier"),
        terminal: w("terminal"),
        customer: w("customer"),
        previous_balance: w("previousBalance"),
        amount: w("amount"),
        stamp_duty: w("stampDuty"),
        stamp_absorbed: w("stampAbsorbed"),
        received: w("received"),
        method: w("method"),
        new_balance: w("newBalance"),
        advance: w("advance"),
        settled: w("settled"),
        in_letters: w("inLetters"),
        thanks: w("thanks"),
    }
}

/// A ledger entry's kind as the statement names it ("Solde d'ouverture", "Vente à crédit"…): the
/// statement's line label, and the settled debt of a versement receipt that has no document
/// number (the paper notebook's opening balance, an adjustment).
pub fn kind_label(lang: Lang, kind: EntryKind) -> String {
    word(lang, &format!("statement.kinds.{}", kind.as_str()))
}

/// The versement receipt of a `payment` ledger entry, in `src.lang`.
pub fn build_settlement(src: &SettlementSource<'_>) -> Result<SettlementReceipt, AppError> {
    let e = src.entry;
    if e.kind != EntryKind::Payment {
        return Err(AppError::validation("entry", "value_invalid"));
    }
    let method = e
        .method
        .ok_or_else(|| AppError::validation("method", "required"))?;
    let overflow = || AppError::validation("amount", "amount_invalid");
    let amount = e.amount.checked_neg().ok_or_else(overflow)?;
    let total_received = amount.checked_add(e.stamp_charged).ok_or_else(overflow)?;
    Ok(SettlementReceipt {
        lang: src.lang,
        rtl: src.lang.is_rtl(),
        number: e.receipt_number.clone().unwrap_or_default(),
        local_time: algiers_datetime(e.created_at)
            .format("%d/%m/%Y %H:%M")
            .to_string(),
        cashier: src.cashier_name.to_owned(),
        terminal_code: src.terminal_code.to_owned(),
        header: receipt_header(src.company, src.lang),
        customer_name: src.customer_name.to_owned(),
        customer_phone: src.customer_phone.map(str::to_owned),
        balance_before: e.balance_after.checked_add(amount).ok_or_else(overflow)?,
        amount,
        stamp_duty: e.stamp_charged,
        stamp_absorbed: e
            .stamp_duty
            .checked_sub(e.stamp_charged)
            .ok_or_else(overflow)?,
        total_received,
        method,
        method_label: method_label(src.lang, method),
        reference: e.reference.clone(),
        balance_after: e.balance_after,
        settled: src.settled.to_vec(),
        total_in_letters: amount_in_letters(src.lang, total_received),
        duplicate: src.duplicate,
        labels: labels(src.lang),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StatementLine {
    /// `dd/mm/yyyy`.
    pub date: String,
    pub label: String,
    pub number: Option<String>,
    pub amount: Money,
    /// The running balance after this entry.
    pub balance: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StatementPrint {
    pub lang: Lang,
    pub rtl: bool,
    pub header: ReceiptHeader,
    pub title: String,
    pub party_name: String,
    pub party_code: String,
    /// `dd/mm/yyyy – dd/mm/yyyy`.
    pub period: String,
    pub opening_label: String,
    pub opening: Money,
    pub lines: Vec<StatementLine>,
    pub closing_label: String,
    pub closing: Money,
    /// `dd/mm/yyyy HH:MM`, Algiers time.
    pub printed_at: String,
}

/// `YYYY-MM-DD` → `dd/mm/yyyy` (unchanged if it is not a date).
fn day(iso: &str) -> String {
    chrono::NaiveDate::parse_from_str(iso, "%Y-%m-%d")
        .map(|d| d.format("%d/%m/%Y").to_string())
        .unwrap_or_else(|_| iso.to_owned())
}

/// The statement for the receipt printer (RPT-037); A4 and PDF come with M3's print host.
pub fn build_statement_print(
    company: &Company,
    party_name: &str,
    party_code: &str,
    statement: &Statement,
    lang: Lang,
    printed_at: i64,
) -> StatementPrint {
    let mut balance = statement.opening;
    let lines = statement
        .entries
        .iter()
        .map(|e| {
            balance = balance.checked_add(e.amount).unwrap_or(balance);
            StatementLine {
                date: day(&e.business_date),
                label: kind_label(lang, e.kind),
                number: e.ref_number.clone().or_else(|| e.receipt_number.clone()),
                amount: e.amount,
                balance,
            }
        })
        .collect();
    StatementPrint {
        lang,
        rtl: lang.is_rtl(),
        header: receipt_header(company, lang),
        title: word(lang, "statement.title"),
        party_name: party_name.to_owned(),
        party_code: party_code.to_owned(),
        period: format!("{} – {}", day(&statement.from), day(&statement.to)),
        opening_label: word(lang, "statement.opening"),
        opening: statement.opening,
        lines,
        closing_label: word(lang, "statement.closing"),
        closing: statement.closing,
        printed_at: algiers_datetime(printed_at)
            .format("%d/%m/%Y %H:%M")
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_core::company::{PriceMode, TaxRegime};

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
            tax_regime: TaxRegime::Reel,
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

    fn entry() -> LedgerEntry {
        LedgerEntry {
            id: "e1".into(),
            party_id: "c1".into(),
            kind: EntryKind::Payment,
            amount: Money(-4_500_000),
            balance_after: Money(500_000),
            method: Some(PaymentMethod::Cash),
            reference: None,
            cheque_bank: None,
            cheque_date: None,
            stamp_duty: Money(67_500),
            stamp_charged: Money(67_500),
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

    #[test]
    fn a_settlement_receipt_carries_the_duty_and_both_balances() {
        let company = company();
        let entry = entry();
        let settled = [SettledLine {
            number: "TK2026-C1-000045".into(),
            amount: Money(4_500_000),
        }];
        let r = build_settlement(&SettlementSource {
            company: &company,
            customer_name: "Karim Bensalah",
            customer_phone: Some("0555123456"),
            entry: &entry,
            settled: &settled,
            cashier_name: "Yacine",
            terminal_code: "C1",
            lang: Lang::Fr,
            duplicate: false,
        })
        .unwrap();
        assert_eq!(r.number, "RV2026-000001");
        assert_eq!(
            (
                r.balance_before,
                r.amount,
                r.stamp_duty,
                r.total_received,
                r.balance_after
            ),
            (
                Money(5_000_000),
                Money(4_500_000),
                Money(67_500),
                Money(4_567_500),
                Money(500_000)
            )
        );
        assert_eq!(r.method_label, "Espèces");
        assert_eq!(r.labels.title, "Reçu de versement");
        assert!(
            r.total_in_letters
                .starts_with("quarante-cinq mille six cent soixante-quinze dinars")
        );
        assert!(!r.rtl);
        let ar = build_settlement(&SettlementSource {
            company: &company,
            customer_name: "كريم",
            customer_phone: None,
            entry: &entry,
            settled: &settled,
            cashier_name: "ياسين",
            terminal_code: "C1",
            lang: Lang::Ar,
            duplicate: true,
        })
        .unwrap();
        assert!(ar.rtl && ar.duplicate);
        assert_eq!(ar.header.name, "مغازة الأمل");
    }

    #[test]
    fn a_statement_print_labels_every_entry() {
        let company = company();
        let mut sale = entry();
        sale.kind = EntryKind::SaleCredit;
        sale.amount = Money(200_000);
        sale.balance_after = Money(700_000);
        sale.ref_number = Some("TK2026-C1-000012".into());
        let statement = Statement {
            party_id: "c1".into(),
            from: "2026-09-01".into(),
            to: "2026-09-30".into(),
            opening: Money(500_000),
            entries: vec![sale],
            debits: Money(200_000),
            credits: Money::ZERO,
            closing: Money(700_000),
        };
        let p = build_statement_print(
            &company,
            "Karim Bensalah",
            "C00001",
            &statement,
            Lang::Fr,
            1_790_593_200_000,
        );
        assert_eq!(p.period, "01/09/2026 – 30/09/2026");
        assert_eq!(p.lines[0].label, "Vente à crédit");
        assert_eq!(p.lines[0].number.as_deref(), Some("TK2026-C1-000012"));
        assert_eq!((p.opening, p.closing), (Money(500_000), Money(700_000)));
    }

    #[test]
    fn an_entry_kind_is_named_in_each_language() {
        assert_eq!(
            kind_label(Lang::Fr, EntryKind::Opening),
            "Solde d'ouverture"
        );
        assert_eq!(kind_label(Lang::Ar, EntryKind::Opening), "رصيد افتتاحي");
        assert_eq!(kind_label(Lang::En, EntryKind::Adjustment), "Adjustment");
    }
}
