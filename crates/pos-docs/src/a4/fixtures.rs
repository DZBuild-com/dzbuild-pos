//! The models the golden HTML tests of the templates render (Task 16), written as JSON under
//! `src/documents/__fixtures__/` so the React side never drifts from the Rust model.

#[cfg(test)]
mod tests {
    use crate::a4::document::build_document;
    use crate::a4::receipt::receipt_page;
    use crate::a4::statement::build_statement;
    use crate::a4::table::build_table;
    use crate::a4::warranty::build_warranty;
    use crate::a4::{statement, table, testdata, warranty};
    use crate::receipt::tests::sample_receipt;
    use crate::sheet::{SheetMeta, range_subtitle, report_title};
    use pos_core::documents::DocType;
    use pos_core::lang::Lang;
    use serde::Serialize;
    use std::path::PathBuf;

    fn dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src/documents/__fixtures__")
    }

    fn pretty<T: Serialize>(model: &T) -> String {
        let mut text = serde_json::to_string_pretty(model).unwrap();
        text.push('\n');
        text
    }

    fn fixtures() -> Vec<(&'static str, String)> {
        let pack = pos_core::datapack::bundled().unwrap();
        let json = |ctx, duplicate| pretty(&build_document(&ctx, &pack, duplicate).unwrap());
        let statement_in = |lang: Lang| {
            let mut seller = testdata::seller();
            if lang == Lang::Ar {
                seller.name = "مغازة الأمل".into();
                seller.activity = Some("تجارة التجزئة للمواد الغذائية العامة".into());
            }
            pretty(&build_statement(
                &seller,
                &pack,
                statement::tests::party(),
                &statement::tests::statement(),
                lang,
                testdata::PRINTED_AT,
            ))
        };
        let table_in = |lang: Lang, shop: &str| {
            let meta = SheetMeta {
                title: report_title(lang, "sales_summary").unwrap(),
                subtitle: Some(range_subtitle(lang, "2026-09-01", "2026-09-30")),
                lang,
                extra_labels: Default::default(),
            };
            pretty(&build_table(
                &table::tests::table(),
                &meta,
                shop,
                Some("000116001234567"),
                testdata::PRINTED_AT,
            ))
        };
        let mut purchase = testdata::quote(DocType::PurchaseOrder);
        purchase.document.number = Some("BC2026-000007".into());
        purchase.document.valid_until = None;
        purchase.counterpart_is_supplier = true;
        let mut transfer = testdata::delivery();
        transfer.document.doc_type = DocType::Transfer;
        transfer.document.number = Some("BT2026-000002".into());
        transfer.counterpart = None;
        transfer.transfer = Some(pos_core::billing::print::TransferInfo {
            from: "Magasin".into(),
            to: "Dépôt Blida".into(),
            driver_name: Some("Rachid Haddad".into()),
            vehicle: Some("12345-116-16".into()),
        });
        vec![
            ("invoice.fr", json(testdata::invoice(), None)),
            ("invoice.ar", json(testdata::invoice_in(Lang::Ar), None)),
            ("invoice.en", json(testdata::invoice_in(Lang::En), None)),
            ("invoice.bilingual", json(testdata::bilingual(), None)),
            ("invoice.ifu", json(testdata::ifu(), None)),
            ("invoice.exempt", json(testdata::exempt(), None)),
            ("invoice.duplicate", json(testdata::invoice(), Some(2))),
            ("invoice.cancelled", json(testdata::cancelled(), None)),
            ("invoice.ticket", json(testdata::from_ticket(), None)),
            ("quote.fr", json(testdata::quote(DocType::Quote), None)),
            (
                "proforma.fr",
                json(testdata::quote(DocType::Proforma), None),
            ),
            ("delivery.fr", json(testdata::delivery(), None)),
            ("credit.fr", json(testdata::credit_note(), None)),
            ("purchase-order.fr", json(purchase, None)),
            ("transfer.fr", json(transfer, None)),
            ("statement.fr", statement_in(Lang::Fr)),
            ("statement.ar", statement_in(Lang::Ar)),
            (
                "receipt.fr",
                pretty(&receipt_page(sample_receipt(Lang::Fr), None)),
            ),
            ("table.fr", table_in(Lang::Fr, "Supérette El Amel")),
            ("table.ar", table_in(Lang::Ar, "مغازة الأمل")),
            (
                "warranty.fr",
                pretty(&build_warranty(
                    &warranty::tests::source(),
                    &testdata::settings(),
                    &pack,
                    Lang::Fr,
                    testdata::PRINTED_AT,
                )),
            ),
        ]
    }

    #[test]
    #[ignore = "writes src/documents/__fixtures__/*.json"]
    fn write_print_fixtures() {
        std::fs::create_dir_all(dir()).unwrap();
        for (name, text) in fixtures() {
            std::fs::write(dir().join(format!("{name}.json")), text).unwrap();
        }
    }

    #[test]
    fn print_fixtures_are_up_to_date() {
        for (name, text) in fixtures() {
            let path = dir().join(format!("{name}.json"));
            let stored = std::fs::read_to_string(&path)
                .unwrap_or_default()
                .replace("\r\n", "\n");
            assert_eq!(
                stored, text,
                "{name}.json is stale: cargo test -p pos-docs write_print_fixtures -- --ignored"
            );
        }
    }
}
