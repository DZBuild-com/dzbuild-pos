//! The words the React templates of `src/documents/` need beyond each model's own labels (M3
//! Task 16): the domain of a printed tag column, the party labels of the statement and of the
//! warranty certificate (its guarantor's RC, décret 13-327 art. 6), and the receipt page's tender
//! names and ticket words.

use super::labels::document_labels;
use super::receipt::receipt_page;
use super::statement::{self, build_statement};
use super::table::build_table;
use super::testdata;
use super::warranty::{self, build_warranty};
use crate::receipt::tests::sample_receipt;
use crate::receipt::{label, method_label};
use crate::sheet::SheetMeta;
use pos_core::lang::Lang;
use pos_core::money::Money;
use pos_core::table::{Cell, ColumnKind, Table, TagDomain};

#[test]
fn a_printed_tag_column_names_its_domain() {
    let mut t = Table::new(&[("method", ColumnKind::Tag), ("amount", ColumnKind::Money)])
        .tag("method", TagDomain::PaymentMethod);
    t.push(vec![Cell::text("cash"), Cell::money(Money(150_000))])
        .unwrap();
    let meta = SheetMeta {
        title: "Encaissements".into(),
        subtitle: None,
        lang: Lang::Fr,
        extra_labels: Default::default(),
    };
    let p = build_table(&t, &meta, "Supérette El Amel", None, testdata::PRINTED_AT);
    assert_eq!(p.columns[0].tag, Some(TagDomain::PaymentMethod));
    assert_eq!(p.columns[1].tag, None);
    assert!(p.tag_labels.contains_key("payment_method.cash"));
    // the shop's NIF on top of every page is labelled
    assert_eq!(p.nif_label, "NIF");
}

#[test]
fn the_archive_summary_labels_the_company_ids() {
    let summary = crate::fiscal_archive::build_summary(
        &pos_core::fiscal::archive::ChainSummary {
            documents: Vec::new(),
            audit: pos_core::audit::ChainReport {
                rows: 0,
                first_broken_seq: None,
                last_seq: 0,
                last_hash: pos_core::audit::GENESIS_HASH.into(),
            },
            closures: Vec::new(),
            receipts: Default::default(),
            till_journals: Vec::new(),
            till_merge_first_broken: None,
        },
        &[],
        crate::fiscal_archive::ArchiveCompany {
            name: "Supérette El Amel".into(),
            nif: Some("000116001234567".into()),
            nis: None,
            rc: Some("16/00-1234567B26".into()),
        },
        "2026-01-01",
        "2026-12-31",
        Lang::Fr,
        testdata::PRINTED_AT,
    );
    let words = document_labels(Lang::Fr);
    assert_eq!(
        (
            summary.labels.nif.as_str(),
            summary.labels.nis.as_str(),
            summary.labels.rc.as_str()
        ),
        (words.nif.as_str(), words.nis.as_str(), words.rc.as_str())
    );
}

#[test]
fn the_statement_and_the_warranty_carry_the_party_labels() {
    let pack = pos_core::datapack::bundled().unwrap();
    let w = build_warranty(
        &warranty::tests::source(),
        &testdata::settings(),
        &pack,
        Lang::Fr,
        testdata::PRINTED_AT,
    );
    assert_eq!(w.party_labels, document_labels(Lang::Fr));
    assert_eq!(w.party_labels.rc, "RC");
    let s = build_statement(
        &testdata::seller(),
        &pack,
        statement::tests::party(),
        &statement::tests::statement(),
        Lang::Ar,
        testdata::PRINTED_AT,
    );
    assert_eq!(s.party_labels, document_labels(Lang::Ar));
}

#[test]
fn a_receipt_page_names_its_tenders_and_carries_the_ticket_words() {
    let page = receipt_page(sample_receipt(Lang::Fr), Some(Lang::Ar));
    let names = |lang| {
        page.receipt
            .payments
            .iter()
            .map(|p| method_label(lang, p.method))
            .collect::<Vec<_>>()
    };
    assert!(!page.receipt.payments.is_empty());
    assert_eq!(page.methods, names(Lang::Fr));
    assert_eq!(page.second_methods, Some(names(Lang::Ar)));
    assert_eq!(page.labels.phone, label(Lang::Fr, "receipt.phone"));
    assert_eq!(page.labels.discount, label(Lang::Fr, "receipt.discount"));
    assert_eq!(page.labels.customer, label(Lang::Fr, "receipt.customer"));
    assert_eq!(
        page.labels.customer_code,
        label(Lang::Fr, "receipt.customerCode")
    );
    assert_eq!(
        page.labels.customer_balance,
        label(Lang::Fr, "receipt.customerBalance")
    );
    assert_eq!(
        receipt_page(sample_receipt(Lang::Fr), None).second_methods,
        None
    );
}
