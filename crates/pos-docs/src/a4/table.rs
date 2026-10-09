//! Any list or report as printed pages (IMX-013: every table exports to PDF): the same labels
//! and tags as the xlsx/csv writer, the shop's name and NIF on each page, RTL for Arabic.

use super::labels::word;
use crate::sheet::{SheetMeta, labels, tag_labels};
use pos_core::lang::Lang;
use pos_core::table::{Cell, ColumnKind, Table, TagDomain};
use serde::Serialize;
use specta::Type;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TableColumnPrint {
    pub key: String,
    pub label: String,
    pub kind: ColumnKind,
    /// The closed list of a `Tag` column: its cells print as `tag_labels["<domain>.<value>"]`.
    pub tag: Option<TagDomain>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TablePrint {
    pub title: String,
    pub subtitle: Option<String>,
    pub lang: Lang,
    pub rtl: bool,
    pub shop: String,
    pub nif: Option<String>,
    /// "NIF", printed before `nif`.
    pub nif_label: String,
    pub columns: Vec<TableColumnPrint>,
    pub rows: Vec<Vec<Cell>>,
    pub totals: Option<Vec<Cell>>,
    /// `"<domain>.<value>"` → label, for `Tag` columns.
    pub tag_labels: BTreeMap<String, String>,
    pub total_label: String,
    /// `dd/mm/yyyy HH:MM`, Algiers time.
    pub printed_at: String,
    /// "Page" (the template adds "n / m").
    pub page_label: String,
}

pub fn build_table(
    table: &Table,
    meta: &SheetMeta,
    shop: &str,
    nif: Option<&str>,
    now: i64,
) -> TablePrint {
    let lang = meta.lang;
    let names = labels(table, lang, &meta.extra_labels);
    TablePrint {
        title: meta.title.clone(),
        subtitle: meta.subtitle.clone(),
        lang,
        rtl: lang.is_rtl(),
        shop: shop.to_owned(),
        nif: nif.map(str::to_owned),
        nif_label: word(lang, "a4.nif"),
        columns: table
            .columns
            .iter()
            .zip(names)
            .map(|(c, label)| TableColumnPrint {
                key: c.key.clone(),
                label,
                kind: c.kind,
                tag: c.tag,
            })
            .collect(),
        rows: table.rows.clone(),
        totals: table.totals.clone(),
        tag_labels: tag_labels(table, lang),
        total_label: word(lang, "sheet.total"),
        printed_at: pos_core::time::algiers_datetime(now)
            .format("%d/%m/%Y %H:%M")
            .to_string(),
        page_label: word(lang, "a4.page"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use pos_core::money::Money;
    use pos_core::table::{Cell, ColumnKind, Table};

    /// One day of the sales summary with its total.
    pub(crate) fn table() -> Table {
        let mut t = Table::new(&[("period", ColumnKind::Date), ("netTtc", ColumnKind::Money)]);
        t.push(vec![
            Cell::text("2026-09-01"),
            Cell::money(Money(1_200_000)),
        ])
        .unwrap();
        t.set_totals(vec![Cell::NULL, Cell::money(Money(1_200_000))])
            .unwrap();
        t
    }

    #[test]
    fn a_table_prints_with_its_labels_totals_and_direction() {
        let t = table();
        let meta = SheetMeta {
            title: "Synthèse des ventes".into(),
            subtitle: Some("Du 01/09/2026 au 30/09/2026".into()),
            lang: Lang::Ar,
            extra_labels: Default::default(),
        };
        let p = build_table(
            &t,
            &meta,
            "مغازة الأمل",
            Some("000116001234567"),
            1_790_586_900_000,
        );
        assert!(p.rtl);
        assert_eq!(p.columns.len(), 2);
        assert_eq!(p.columns[1].kind, ColumnKind::Money);
        assert!(p.totals.is_some());
        assert_eq!(
            p.total_label,
            crate::locales::text("ar", "sheet.total").unwrap()
        );
    }
}
