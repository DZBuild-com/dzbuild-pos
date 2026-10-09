//! The A4 statement of a customer's or a supplier's account (CRD-005, RPT-037, PUR-006): debits
//! and credits in two columns, the running balance, opening and closing (M2 builds the thermal
//! one; this is its page and PDF).

use super::document::{PartyBlock, day, party_block};
use super::labels::{DocumentLabels, document_labels, fill, word};
use pos_core::datapack::Pack;
use pos_core::documents::DocumentParty;
use pos_core::lang::Lang;
use pos_core::ledger::Statement;
use pos_core::money::Money;
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StatementParty {
    pub name: String,
    pub code: String,
    pub address: Option<String>,
    pub nif: Option<String>,
    pub rc: Option<String>,
    pub is_supplier: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StatementLine {
    /// `dd/mm/yyyy`.
    pub date: String,
    pub label: String,
    pub number: Option<String>,
    pub debit: Money,
    pub credit: Money,
    /// The running balance after this entry.
    pub balance: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StatementLabels {
    pub date: String,
    pub label: String,
    pub number: String,
    pub debit: String,
    pub credit: String,
    pub balance: String,
    pub opening: String,
    pub closing: String,
    pub period: String,
    pub printed_at: String,
    pub customer: String,
    pub supplier: String,
    pub code: String,
    pub nif: String,
    pub rc: String,
    pub address: String,
    pub total: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StatementA4 {
    pub lang: Lang,
    pub rtl: bool,
    pub title: String,
    pub seller: PartyBlock,
    pub party: StatementParty,
    /// `dd/mm/yyyy – dd/mm/yyyy`.
    pub period: String,
    pub opening: Money,
    pub lines: Vec<StatementLine>,
    pub total_debit: Money,
    pub total_credit: Money,
    pub closing: Money,
    pub labels: StatementLabels,
    /// The words of the seller's card (legal form, RC, NIF, NIS, RIB…) and of the page numbers.
    pub party_labels: DocumentLabels,
    /// `dd/mm/yyyy HH:MM`, Algiers time.
    pub printed_at: String,
}

/// The page of `statement` in `lang`: positive entries in the debit column, negative ones in the
/// credit column, the balance after each (M2's sign: what the customer owes, what is owed to the
/// supplier).
pub fn build_statement(
    seller: &DocumentParty,
    pack: &Pack,
    party: StatementParty,
    statement: &Statement,
    lang: Lang,
    now: i64,
) -> StatementA4 {
    let l = |k: &str| word(lang, &format!("statementA4.{k}"));
    let mut balance = statement.opening;
    let lines = statement
        .entries
        .iter()
        .map(|e| {
            balance = balance.checked_add(e.amount).unwrap_or(balance);
            let (debit, credit) = if e.amount.is_negative() {
                (Money::ZERO, e.amount.checked_neg().unwrap_or(Money::ZERO))
            } else {
                (e.amount, Money::ZERO)
            };
            StatementLine {
                date: day(&e.business_date),
                label: word(lang, &format!("statement.kinds.{}", e.kind.as_str())),
                number: e.ref_number.clone().or_else(|| e.receipt_number.clone()),
                debit,
                credit,
                balance,
            }
        })
        .collect();
    StatementA4 {
        lang,
        rtl: lang.is_rtl(),
        title: word(lang, "doc.statement"),
        seller: party_block(seller, pack, lang),
        period: fill(
            lang,
            "statementA4.range",
            &[("from", &day(&statement.from)), ("to", &day(&statement.to))],
        ),
        opening: statement.opening,
        lines,
        total_debit: statement.debits,
        total_credit: statement.credits,
        closing: statement.closing,
        labels: StatementLabels {
            date: l("date"),
            label: l("label"),
            number: l("number"),
            debit: l("debit"),
            credit: l("credit"),
            balance: l("balance"),
            opening: l("opening"),
            closing: l("closing"),
            period: l("period"),
            printed_at: word(lang, "a4.printedAt"),
            customer: word(lang, "a4.customer"),
            supplier: word(lang, "a4.supplier"),
            code: l("code"),
            nif: word(lang, "a4.nif"),
            rc: word(lang, "a4.rc"),
            address: word(lang, "a4.address"),
            total: word(lang, "sheet.total"),
        },
        party_labels: document_labels(lang),
        party,
        printed_at: pos_core::time::algiers_datetime(now)
            .format("%d/%m/%Y %H:%M")
            .to_string(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::a4::testdata;
    use pos_core::ledger::{EntryKind, LedgerEntry, Statement};

    pub(crate) fn party() -> StatementParty {
        StatementParty {
            name: "SARL Amine".into(),
            code: "C00012".into(),
            address: Some("5 rue Didouche Mourad, Alger".into()),
            nif: Some("000016001234567".into()),
            rc: None,
            is_supplier: false,
        }
    }

    pub(crate) fn statement() -> Statement {
        let entry = |id: &str, day: &str, kind: EntryKind, amount: i64, number: Option<&str>| {
            let mut e = LedgerEntry::for_tests(id, "c-amine", kind, Money(amount), day);
            e.ref_number = number.map(str::to_owned);
            e
        };
        Statement {
            party_id: "c-amine".into(),
            from: "2026-09-01".into(),
            to: "2026-09-30".into(),
            opening: Money(150_000),
            entries: vec![
                entry(
                    "e1",
                    "2026-09-05",
                    EntryKind::SaleCredit,
                    2_280_000,
                    Some("FA2026-000045"),
                ),
                entry("e2", "2026-09-12", EntryKind::Payment, -1_000_000, None),
                entry(
                    "e3",
                    "2026-09-20",
                    EntryKind::Refund,
                    -339_150,
                    Some("AV2026-000003"),
                ),
            ],
            debits: Money(2_280_000),
            credits: Money(1_339_150),
            closing: Money(1_090_850),
        }
    }

    #[test]
    fn a_statement_splits_debits_and_credits_and_runs_the_balance() {
        let pack = pos_core::datapack::bundled().unwrap();
        let s = build_statement(
            &testdata::seller(),
            &pack,
            party(),
            &statement(),
            Lang::Fr,
            testdata::PRINTED_AT,
        );
        assert_eq!(s.title, "Relevé de compte");
        assert_eq!(s.period, "01/09/2026 – 30/09/2026");
        assert_eq!(
            s.lines
                .iter()
                .map(|l| (l.debit, l.credit, l.balance))
                .collect::<Vec<_>>(),
            [
                (Money(2_280_000), Money::ZERO, Money(2_430_000)),
                (Money::ZERO, Money(1_000_000), Money(1_430_000)),
                (Money::ZERO, Money(339_150), Money(1_090_850)),
            ]
        );
        assert_eq!(
            (s.total_debit, s.total_credit, s.closing),
            (Money(2_280_000), Money(1_339_150), Money(1_090_850))
        );
        assert_eq!(s.lines[0].number.as_deref(), Some("FA2026-000045"));
    }
}
