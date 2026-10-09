//! Cash reports (CSH-007: X at any time, Z at the close, both "conformes"), the printer test
//! page (ONB-009, HW-009) and the label printer's test label, in the chosen language.

use crate::error::HwError;
use crate::escpos::{Builder, Justify};
use crate::fmt::{amount, datetime, rate};
use crate::labels::model::{
    LabelBarcode, LabelData, LabelFields, LabelGroup, LabelLang, LabelSymbology, LabelTemplate,
    SymbologyChoice,
};
use crate::layout::{Align, Layout, Row, pair, strong, strong_pair, text};
use crate::profile::{CodePage, PaperWidth, Profile, RenderMode};
use crate::raster::Typesetter;
use crate::render::{DrawerKick, Job, Rendered, Segment, barcode, finish, segments};
use pos_core::cash::closing::{SessionSummary, ZReport};
use pos_core::lang::Lang;
use pos_core::money::Money;
use pos_docs::receipt::{label, method_label};

fn heading(shop: &str, title: String) -> Vec<Row> {
    vec![
        Row::Text {
            text: shop.to_owned(),
            align: Align::Center,
            large: true,
            bold: true,
        },
        strong(title, Align::Center),
        Row::Rule,
    ]
}

fn summary_rows(s: &SessionSummary, lang: Lang) -> Vec<Row> {
    let l = |key: &str| label(lang, key);
    let a = |m: Money| amount(m, lang);
    let mut rows = vec![
        pair(l("receipt.terminal"), s.terminal_code.clone()),
        pair(l("receipt.cashier"), s.cashier_name.clone()),
        pair(l("z.opened"), datetime(s.session.opened_at)),
        Row::Rule,
        pair(l("z.tickets"), s.tickets.to_string()),
        pair(l("z.returns"), s.returns.to_string()),
    ];
    if let Some(first) = &s.first_ticket {
        rows.push(pair(l("z.firstTicket"), first.clone()));
    }
    if let Some(last) = &s.last_ticket {
        rows.push(pair(l("z.lastTicket"), last.clone()));
    }
    if s.blind {
        // Blind close (CSH-004): the counts and the non-cash tenders only.
        rows.push(Row::Rule);
        rows.push(strong(l("z.byMethod"), Align::Start));
        for m in &s.by_method {
            rows.push(pair(
                format!("{} ({})", method_label(lang, m.method), m.count),
                a(m.net),
            ));
        }
        return rows;
    }
    rows.push(pair(l("z.sales"), a(s.sales_ttc)));
    rows.push(pair(l("z.returns"), a(s.returns_ttc)));
    // M3: the invoices and credit notes of the session, whose payments are in the methods below.
    if s.invoices > 0 {
        rows.push(pair(
            format!("{} ({})", l("z.invoices"), s.invoices),
            a(s.invoices_ttc),
        ));
    }
    if s.credit_notes > 0 {
        rows.push(pair(
            format!("{} ({})", l("z.creditNotes"), s.credit_notes),
            a(s.credit_notes_ttc),
        ));
    }
    rows.push(strong_pair(l("z.net"), a(s.net_ttc)));
    if s.discount_total != Money::ZERO {
        rows.push(pair(l("z.discounts"), a(s.discount_total)));
    }
    if s.show_tax {
        for t in &s.per_rate {
            rows.push(pair(
                format!("{} {}", l("receipt.vat"), rate(t.rate_bp, lang)),
                a(t.tax),
            ));
        }
    }
    if s.ifu_mention {
        rows.push(text(l("receipt.ifuMention"), Align::Center));
    }
    if s.stamp_charged != Money::ZERO {
        rows.push(pair(l("receipt.stampDuty"), a(s.stamp_charged)));
    }
    if s.stamp_absorbed != Money::ZERO {
        rows.push(pair(l("receipt.stampAbsorbed"), a(s.stamp_absorbed)));
    }
    if s.rounding != Money::ZERO {
        rows.push(pair(l("receipt.rounding"), a(s.rounding)));
    }
    rows.push(Row::Rule);
    rows.push(strong(l("z.byMethod"), Align::Start));
    for m in &s.by_method {
        rows.push(pair(
            format!("{} ({})", method_label(lang, m.method), m.count),
            a(m.net),
        ));
    }
    rows.push(Row::Rule);
    rows.push(pair(l("z.float"), a(s.opening_float)));
    rows.push(pair(l("z.cashSales"), a(s.cash_sales)));
    rows.push(pair(l("z.refunds"), a(s.cash_refunds)));
    rows.push(pair(l("z.paidIn"), a(s.paid_in)));
    rows.push(pair(l("z.paidOut"), a(s.paid_out)));
    rows.push(pair(l("z.drops"), a(s.drops)));
    for e in s.expenses.iter().filter(|e| e.amount != Money::ZERO) {
        rows.push(pair(
            l(&format!("z.categories.{}", e.category)),
            a(e.amount),
        ));
    }
    rows.push(pair(l("z.expenses"), a(s.expenses_total)));
    // CRD-004: kridi versements taken in cash are in the expected cash
    if s.credit_payments != Money::ZERO {
        rows.push(pair(l("z.creditPayments"), a(s.credit_payments)));
    }
    if let Some(expected) = s.expected_cash {
        rows.push(strong_pair(l("z.expected"), a(expected)));
    }
    rows
}

/// The Z of a closed session. `approver` is the display name of the manager who accepted the
/// variance: `z.approved_by` holds their user id, which is never printed, so the caller resolves
/// the name (the grant's `approver_name` at the close, the user record on a reprint). The line
/// prints only when the Z records an approval and a name is given.
pub fn z_layout(z: &ZReport, shop: &str, lang: Lang, approver: Option<&str>) -> Layout {
    let l = |key: &str| label(lang, key);
    let a = |m: Money| amount(m, lang);
    let mut rows = heading(shop, l("z.title"));
    rows.push(pair(l("z.number"), z.z_number.clone()));
    // LAN-003: closed on a till with operations not yet sent to the main PC
    if let Some(pending) = z.offline_pending {
        rows.push(strong_pair(l("z.offline"), pending.to_string()));
    }
    rows.extend(summary_rows(&z.summary, lang));
    rows.push(Row::Rule);
    rows.push(strong(l("z.count"), Align::Start));
    for c in z.counts.iter().filter(|c| c.count > 0) {
        let subtotal = Money(c.denomination.0.saturating_mul(c.count));
        rows.push(pair(
            format!("{} × {}", a(c.denomination), c.count),
            a(subtotal),
        ));
    }
    rows.push(strong_pair(l("z.counted"), a(z.counted_cash)));
    rows.push(strong_pair(l("z.variance"), a(z.variance)));
    if let (Some(_), Some(name)) = (&z.approved_by, approver) {
        rows.push(pair(l("z.approvedBy"), name));
    }
    rows.push(Row::Rule);
    rows.push(strong_pair(l("z.grandTotal"), a(z.grand_total)));
    let intact = z.audit_anchor.intact && z.document_anchors.iter().all(|d| d.intact);
    rows.push(pair(
        l("z.journal"),
        l(if intact { "z.intact" } else { "z.broken" }),
    ));
    rows.push(text(
        z.hash.chars().take(16).collect::<String>(),
        Align::Center,
    ));
    rows.push(pair(l("z.closed"), datetime(z.closed_at)));
    rows.push(pair(l("receipt.cashier"), z.closed_by_name.clone()));
    if let Some(notes) = &z.notes {
        rows.push(text(notes.clone(), Align::Start));
    }
    Layout {
        rtl: lang == Lang::Ar,
        rows,
    }
}

pub fn x_layout(summary: &SessionSummary, shop: &str, lang: Lang, printed_at: i64) -> Layout {
    let mut rows = heading(shop, label(lang, "z.xTitle"));
    rows.push(pair(label(lang, "z.printedAt"), datetime(printed_at)));
    rows.extend(summary_rows(summary, lang));
    Layout {
        rtl: lang == Lang::Ar,
        rows,
    }
}

/// Title, printer, paper and mode as a raster block with an Arabic sample, then a text-mode
/// line with the accents of the code page, a barcode and the cut: one page proves the three.
pub fn test_page(
    profile: &Profile,
    printer: &str,
    lang: Lang,
    ts: &mut Typesetter,
) -> Result<Rendered, HwError> {
    profile.validate()?;
    let l = |key: &str| label(lang, key);
    let paper = match profile.width {
        PaperWidth::Mm58 => "58 mm",
        PaperWidth::Mm80 => "80 mm",
    };
    let mode = match profile.render {
        RenderMode::Auto => "auto",
        RenderMode::Text => "text",
        RenderMode::Raster => "raster",
    };
    let page = match profile.code_page {
        CodePage::Pc858 => "pc858",
        CodePage::Wpc1252 => "wpc1252",
    };
    let layout = Layout {
        rtl: lang == Lang::Ar,
        rows: vec![
            Row::Text {
                text: l("test.title"),
                align: Align::Center,
                large: true,
                bold: true,
            },
            pair(l("test.printer"), printer),
            pair(l("test.paper"), format!("{paper} · {}", profile.dots)),
            pair(l("test.mode"), l(&format!("test.modes.{mode}"))),
            pair(l("test.codePage"), l(&format!("test.codePages.{page}"))),
            Row::Rule,
            text(label(Lang::Ar, "test.arabicSample"), Align::Center),
            text(l("test.ok"), Align::Center),
            Row::Rule,
        ],
    };
    let mut b = Builder::new();
    b.init(profile.exit_chinese_mode);
    let job = Job {
        layout: &layout,
        logo: None,
        drawer: DrawerKick::None,
    };
    for segment in segments(&job, profile, ts) {
        if let Segment::Image(image) = segment {
            b.raster(&image, profile.band_rows)?;
        }
    }
    b.code_page(profile.code_page_selector)
        .justify(Justify::Left);
    b.line(
        &profile
            .code_page
            .encode("àâçéèêëîïôùûü ÀÉÈÇ € × 0123456789"),
    );
    b.line(&vec![b'-'; usize::from(profile.columns)]);
    barcode(&mut b, "DZPOS-TEST-0123", profile)?;
    finish(&mut b, profile, DrawerKick::None);
    Ok(Rendered {
        bytes: b.into_bytes(),
        raster: true,
    })
}

/// A 40×25 test label (fits every common roll): the Arabic sample, "Test label · <printer>" in
/// the chosen language, and an EAN-13 of the in-store range.
pub fn label_test_group(printer: &str, lang: Lang) -> LabelGroup {
    let label_lang = match lang {
        Lang::Ar => LabelLang::Ar,
        Lang::Fr => LabelLang::ArFr,
        Lang::En => LabelLang::ArEn,
    };
    let template = LabelTemplate {
        id: "test".into(),
        name: String::new(),
        width_dmm: 400,
        height_dmm: 250,
        gap_dmm: 20,
        columns: 1,
        column_gap_dmm: 0,
        lang: label_lang,
        fields: LabelFields {
            name: true,
            barcode: true,
            ..LabelFields::default()
        },
        symbology: SymbologyChoice::Auto,
        name_lines: 1,
    };
    let title = crate::labels::words::word(lang, "test.labelTitle");
    let sample = LabelData {
        product_id: None,
        copies: 1,
        lang: label_lang,
        name: label(Lang::Ar, "test.arabicSample"),
        name_second: Some(format!("{title} · {printer}")),
        reference: None,
        price: Money::ZERO,
        old_price: None,
        unit_code: "pc".into(),
        unit_price: None,
        weight: None,
        lot_number: None,
        expiry: None,
        packed_on: None,
        shop_name: None,
        barcode: Some(LabelBarcode {
            symbology: LabelSymbology::Ean13,
            data: "2000000000008".into(),
        }),
        demo: false,
        jewel: None,
    };
    LabelGroup {
        template,
        labels: vec![sample],
    }
}

#[cfg(test)]
mod offline_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::escpos::dump;
    use crate::profile::{PaperWidth, Preset, Profile};
    use crate::raster::Typesetter;
    use crate::render::{DrawerKick, Job, render};
    use crate::testdata::z_report;

    #[test]
    fn z_report_fr_80() {
        let layout = z_layout(&z_report(), "Supérette El Amel", Lang::Fr, None);
        let texts: Vec<String> = layout.texts().map(str::to_owned).collect();
        assert!(texts.iter().any(|t| t == "Rapport Z"));
        assert!(texts.iter().any(|t| t == "ZC1-000031"));
        assert!(texts.iter().any(|t| t == "Intègre"));
        let out = render(
            &Job {
                layout: &layout,
                logo: None,
                drawer: DrawerKick::None,
            },
            &Profile::preset(Preset::Generic, PaperWidth::Mm80),
            &mut Typesetter::new(),
        )
        .unwrap();
        assert!(!out.raster);
        insta::assert_snapshot!(dump(&out.bytes));
    }

    #[test]
    fn x_report_hides_what_the_summary_hides() {
        let mut summary = z_report().summary;
        summary.expected_cash = None; // no cash.view_expected
        let layout = x_layout(&summary, "Supérette El Amel", Lang::En, 1_790_593_200_000);
        let texts: Vec<String> = layout.texts().map(str::to_owned).collect();
        assert!(texts.iter().any(|t| t == "X report"));
        assert!(!texts.iter().any(|t| t == "Expected cash"));
    }

    /// A blind X (CSH-004) prints the counts and the non-cash tenders, no drawer line.
    #[test]
    fn a_blind_x_report_prints_no_drawer_figure() {
        let mut summary = z_report().summary;
        summary.blind = true;
        summary.expected_cash = None;
        let full: Vec<String> = x_layout(&z_report().summary, "Shop", Lang::Fr, 0)
            .texts()
            .map(str::to_owned)
            .collect();
        let blind: Vec<String> = x_layout(&summary, "Shop", Lang::Fr, 0)
            .texts()
            .map(str::to_owned)
            .collect();
        for key in [
            "z.float",
            "z.cashSales",
            "z.refunds",
            "z.paidIn",
            "z.paidOut",
            "z.drops",
            "z.expenses",
            "z.sales",
            "z.net",
        ] {
            let text = label(Lang::Fr, key);
            assert!(full.contains(&text), "{key} on the full X");
            assert!(!blind.contains(&text), "{key} on the blind X");
        }
        assert!(blind.contains(&label(Lang::Fr, "z.tickets")));
    }

    #[test]
    fn an_ifu_z_prints_the_mention_and_no_tva() {
        let mut z = z_report();
        z.summary.show_tax = false;
        z.summary.ifu_mention = true;
        let texts: Vec<String> = z_layout(&z, "Shop", Lang::Fr, None)
            .texts()
            .map(str::to_owned)
            .collect();
        assert!(texts.iter().any(|t| t == "Régime IFU – TVA non applicable"));
        let vat = label(Lang::Fr, "receipt.vat");
        assert!(!texts.iter().any(|t| t.starts_with(&vat)), "{texts:#?}");
        let reel: Vec<String> = z_layout(&z_report(), "Shop", Lang::Fr, None)
            .texts()
            .map(str::to_owned)
            .collect();
        assert!(reel.iter().any(|t| t.starts_with(&vat)));
    }

    /// CRD-004: the kridi versements taken in cash count in the expected cash, so the X and the Z
    /// print them as a drawer line of their own, right after the expenses and so just before
    /// "Espèces attendues" (the drawer lines on paper add up to it); none when there were none.
    #[test]
    fn the_cash_versements_are_a_drawer_line_before_the_expected_cash() {
        let mut z = z_report();
        z.summary.credit_payments = Money(202_000);
        z.summary.expected_cash = Some(Money(8_140_250));
        let mut withheld = z.summary.clone();
        withheld.expected_cash = None;
        for (layout, expected) in [
            (z_layout(&z, "Shop", Lang::Fr, None), true),
            (x_layout(&z.summary, "Shop", Lang::Fr, 0), true),
            (x_layout(&withheld, "Shop", Lang::Fr, 0), false),
        ] {
            let pairs: Vec<(&str, &str)> = layout
                .rows
                .iter()
                .filter_map(|row| match row {
                    Row::Pair { start, end, .. } => Some((start.as_str(), end.as_str())),
                    _ => None,
                })
                .collect();
            let at = |key: &str| {
                let wanted = label(Lang::Fr, key);
                pairs.iter().position(|(start, _)| *start == wanted)
            };
            let line = at("z.creditPayments").expect("the versements line");
            assert_eq!(pairs[line].1, "2 020,00");
            assert_eq!(at("z.expenses"), Some(line - 1), "{pairs:#?}");
            assert_eq!(at("z.expected").is_some(), expected);
            if expected {
                assert_eq!(at("z.expected"), Some(line + 1), "{pairs:#?}");
            }
        }
        let ar = x_layout(&z.summary, "Shop", Lang::Ar, 0);
        assert!(ar.texts().any(|t| t == "دفعات الكريدي نقداً"));
        let none: Vec<String> = z_layout(&z_report(), "Shop", Lang::Fr, None)
            .texts()
            .map(str::to_owned)
            .collect();
        assert!(!none.contains(&label(Lang::Fr, "z.creditPayments")));
        let mut blind = z.summary.clone();
        blind.blind = true;
        let blind: Vec<String> = x_layout(&blind, "Shop", Lang::Fr, 0)
            .texts()
            .map(str::to_owned)
            .collect();
        assert!(
            !blind.contains(&label(Lang::Fr, "z.creditPayments")),
            "a blind X adds nothing up"
        );
    }

    #[test]
    fn z_report_ar_is_rtl() {
        assert!(z_layout(&z_report(), "سوبيرات الأمل", Lang::Ar, None).rtl);
    }

    /// `approved_by` is the manager's user id: the Z prints the name the caller resolved for it,
    /// never the id.
    #[test]
    fn an_approved_variance_prints_the_manager_by_name() {
        let mut z = z_report();
        z.approved_by = Some("0192a3b4-c5d6-7e8f-9a0b-1c2d3e4f5a6b".into());
        let texts: Vec<String> = z_layout(&z, "Supérette El Amel", Lang::Fr, Some("Amine Benali"))
            .texts()
            .map(str::to_owned)
            .collect();
        let at = texts.iter().position(|t| t == "Écart validé par").unwrap();
        assert_eq!(texts[at + 1], "Amine Benali");
        assert!(!texts.iter().any(|t| t.contains("0192a3b4")), "{texts:#?}");
    }

    #[test]
    fn a_z_never_prints_the_approver_id() {
        let mut z = z_report();
        z.approved_by = Some("0192a3b4-c5d6-7e8f-9a0b-1c2d3e4f5a6b".into());
        let texts: Vec<String> = z_layout(&z, "Supérette El Amel", Lang::Fr, None)
            .texts()
            .map(str::to_owned)
            .collect();
        assert!(!texts.iter().any(|t| t.contains("0192a3b4")), "{texts:#?}");
        let unapproved: Vec<String> = z_layout(
            &z_report(),
            "Supérette El Amel",
            Lang::Fr,
            Some("Amine Benali"),
        )
        .texts()
        .map(str::to_owned)
        .collect();
        assert!(
            !unapproved
                .iter()
                .any(|t| t == "Écart validé par" || t == "Amine Benali"),
            "no approval on record, none printed"
        );
    }

    #[test]
    fn the_test_label_names_its_printer_in_every_language() {
        for (lang, title) in [
            (Lang::Fr, "Étiquette de test · XP-365B"),
            (Lang::En, "Test label · XP-365B"),
            (Lang::Ar, "ملصق تجريبي · XP-365B"),
        ] {
            let group = label_test_group("XP-365B", lang);
            let [sample] = group.labels.as_slice() else {
                panic!("one test label");
            };
            assert_eq!(sample.name, label(Lang::Ar, "test.arabicSample"));
            assert_eq!(sample.name_second.as_deref(), Some(title), "{lang:?}");
            assert_eq!(
                (group.template.width_dmm, group.template.height_dmm),
                (400, 250)
            );
        }
    }

    #[test]
    fn test_page_has_a_raster_and_a_text_part() {
        let profile = Profile::preset(Preset::Xprinter, PaperWidth::Mm58);
        let out = test_page(&profile, "XP-58", Lang::Fr, &mut Typesetter::new()).unwrap();
        let bytes = &out.bytes;
        assert!(
            bytes.windows(3).any(|w| w == [0x1D, b'v', b'0']),
            "Arabic sample as raster"
        );
        assert!(
            bytes.windows(3).any(|w| w == [0x1B, b't', 19]),
            "code page selected for the text part"
        );
        assert!(
            bytes.windows(3).any(|w| w == b"\x85\x83\x87"),
            "àâç in PC858"
        );
        assert!(bytes.windows(2).any(|w| w == [0x1D, b'k']), "barcode");
        assert!(bytes.ends_with(&[0x1D, b'V', 66, 0]));
    }
}
