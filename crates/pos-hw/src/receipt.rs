//! The receipt's rows (DOC-005 mentions, DOC-013 bilingual, DOC-014 logo/header/footer,
//! DOC-016 number barcode, DOC-018 DUPLICATA, PRM-006 "you saved") in the print language, with
//! the second language's words after each label when the shop prints bilingual receipts.

use crate::fmt::{amount, currency, date, qty, rate};
use crate::layout::{Align, Layout, Row, pair, strong, strong_pair, text};
use pos_core::documents::DocType;
use pos_core::lang::Lang;
use pos_core::money::Money;
use pos_core::payments::PaymentMethod;
use pos_docs::receipt::{Receipt, ReceiptLabels, label, labels, method_label};

struct Words {
    lang: Lang,
    second: Option<Lang>,
    main: ReceiptLabels,
    other: Option<ReceiptLabels>,
}

impl Words {
    fn new(lang: Lang, second: Option<Lang>, doc_type: DocType) -> Words {
        let second = second.filter(|s| *s != lang);
        Words {
            lang,
            second,
            main: labels(lang, doc_type),
            other: second.map(|s| labels(s, doc_type)),
        }
    }

    fn get(&self, pick: impl Fn(&ReceiptLabels) -> &String) -> String {
        match &self.other {
            Some(other) => format!("{} / {}", pick(&self.main), pick(other)),
            None => pick(&self.main).clone(),
        }
    }

    fn key(&self, key: &str) -> String {
        match self.second {
            Some(second) => format!("{} / {}", label(self.lang, key), label(second, key)),
            None => label(self.lang, key),
        }
    }

    fn method(&self, method: PaymentMethod) -> String {
        match self.second {
            Some(second) => format!(
                "{} / {}",
                method_label(self.lang, method),
                method_label(second, method)
            ),
            None => method_label(self.lang, method),
        }
    }
}

pub fn receipt_layout(r: &Receipt, second: Option<Lang>) -> Layout {
    let lang = r.lang;
    let w = Words::new(lang, second, r.doc_type);
    let h = &r.header;
    let mut rows = vec![
        Row::Logo,
        Row::Text {
            text: h.name.clone(),
            align: Align::Center,
            large: true,
            bold: true,
        },
    ];
    for value in [&h.legal_form, &h.address].into_iter().flatten() {
        rows.push(text(value.clone(), Align::Center));
    }
    if let Some(phone) = &h.phone {
        rows.push(text(
            format!("{} {phone}", w.key("receipt.phone")),
            Align::Center,
        ));
    }
    for (value, name) in [
        (&h.rc, w.get(|l| &l.rc)),
        (&h.nif, w.get(|l| &l.nif)),
        (&h.nis, w.get(|l| &l.nis)),
        (&h.ai, w.get(|l| &l.ai)),
    ] {
        if let Some(value) = value {
            rows.push(text(format!("{name} {value}"), Align::Center));
        }
    }
    if let Some(header) = &h.header_text {
        rows.push(text(header.clone(), Align::Center));
    }
    rows.push(Row::Rule);
    rows.push(strong(w.get(|l| &l.title), Align::Center));
    // Finding 39: a ticket of the demo shop can never pass for a real one.
    if h.demo {
        rows.push(Row::Text {
            text: w.key("receipt.demo"),
            align: Align::Center,
            large: true,
            bold: true,
        });
    }
    if r.duplicate {
        rows.push(Row::Text {
            text: w.get(|l| &l.duplicate),
            align: Align::Center,
            large: true,
            bold: true,
        });
    }
    rows.push(text(
        format!("{} {}", w.get(|l| &l.number), r.number),
        Align::Start,
    ));
    rows.push(pair(
        format!("{} {}", w.get(|l| &l.date), r.local_time),
        format!("{} {}", w.get(|l| &l.terminal), r.terminal_code),
    ));
    rows.push(text(
        format!("{} {}", w.get(|l| &l.cashier), r.cashier),
        Align::Start,
    ));
    // DOC-015: the customer by name and code only; his phone and address stay off the paper
    if let Some(customer) = &r.customer {
        rows.push(text(
            format!("{} {}", w.key("receipt.customer"), customer.name),
            Align::Start,
        ));
        rows.push(text(
            format!("{} {}", w.key("receipt.customerCode"), customer.code),
            Align::Start,
        ));
    }
    if let Some(original) = &r.original_number {
        rows.push(text(
            format!("{} {original}", w.get(|l| &l.original)),
            Align::Start,
        ));
    }
    // RST-009: a restaurant ticket says how it was served, with the takeaway or delivery number
    // the customer is called by in large type, else the table and its covers
    if let Some(dining) = &r.dining {
        rows.push(Row::Rule);
        rows.push(strong(dining.mode.clone(), Align::Center));
        match (dining.order_no, &dining.place) {
            (Some(n), _) => rows.push(Row::Text {
                text: label(lang, "receipt.dining.order").replace("{n}", &n.to_string()),
                align: Align::Center,
                large: true,
                bold: true,
            }),
            (None, Some(place)) => {
                let covers = (dining.covers > 0)
                    .then(|| label(lang, "slip.covers").replace("{n}", &dining.covers.to_string()));
                let shown = match covers {
                    Some(covers) => format!("{place} — {covers}"),
                    None => place.clone(),
                };
                rows.push(text(shown, Align::Center));
            }
            (None, None) => {}
        }
    }
    rows.push(Row::Rule);
    for line in &r.lines {
        rows.push(text(line.description.clone(), Align::Start));
        let unit = if line.unit_code == "pc" {
            String::new()
        } else {
            format!(" {}", line.unit_code)
        };
        rows.push(pair(
            format!(
                "  {}{unit} × {}",
                qty(line.qty, lang),
                amount(line.unit_price, lang)
            ),
            amount(line.total, lang),
        ));
        if line.discount != Money::ZERO {
            let shown = line.discount.checked_neg().unwrap_or(line.discount);
            rows.push(pair(
                format!("  {}", w.key("receipt.discount")),
                amount(shown, lang),
            ));
        }
        for component in &line.components {
            rows.push(text(
                format!(
                    "   - {} × {}",
                    component.description,
                    qty(component.qty, lang)
                ),
                Align::Start,
            ));
        }
        if let Some(note) = &line.note {
            rows.push(text(format!("   {note}"), Align::Start));
        }
    }
    rows.push(Row::Rule);
    rows.push(pair(w.get(|l| &l.items), qty(r.item_count, lang)));
    if r.show_tax {
        rows.push(pair(w.get(|l| &l.subtotal_ht), amount(r.subtotal_ht, lang)));
        for tax in &r.taxes {
            rows.push(pair(
                format!(
                    "{} {} ({})",
                    w.get(|l| &l.vat),
                    rate(tax.rate_bp, lang),
                    amount(tax.base_ht, lang)
                ),
                amount(tax.tax, lang),
            ));
        }
    }
    rows.push(strong_pair(
        w.get(|l| &l.total_ttc),
        amount(r.total_ttc, lang),
    ));
    if r.stamp_duty != Money::ZERO {
        rows.push(pair(w.get(|l| &l.stamp_duty), amount(r.stamp_duty, lang)));
    }
    if r.rounding != Money::ZERO {
        rows.push(pair(w.get(|l| &l.rounding), amount(r.rounding, lang)));
    }
    let due = if r.doc_type == DocType::ReturnNote {
        w.get(|l| &l.refund)
    } else {
        w.get(|l| &l.net_to_pay)
    };
    rows.push(Row::Pair {
        start: due,
        end: format!("{} {}", amount(r.total_due, lang), currency(lang)),
        large: true,
        bold: true,
    });
    if r.stamp_absorbed != Money::ZERO {
        rows.push(text(
            format!(
                "{} : {}",
                w.get(|l| &l.stamp_absorbed),
                amount(r.stamp_absorbed, lang)
            ),
            Align::Start,
        ));
    }
    rows.push(Row::Rule);
    for payment in &r.payments {
        rows.push(pair(w.method(payment.method), amount(payment.amount, lang)));
        if let Some(reference) = &payment.reference {
            rows.push(text(format!("  {reference}"), Align::Start));
        }
        // PAY-023: a cheque is exempt from the duty with its bank and date on the receipt
        if let Some(bank) = &payment.cheque_bank {
            rows.push(text(
                format!("  {} {bank}", w.get(|l| &l.cheque_bank)),
                Align::Start,
            ));
        }
        if let Some(day) = &payment.cheque_date {
            rows.push(text(
                format!("  {} {}", w.get(|l| &l.cheque_date), date(day)),
                Align::Start,
            ));
        }
    }
    if r.change != Money::ZERO {
        rows.push(strong_pair(w.get(|l| &l.change), amount(r.change, lang)));
    }
    // SAL-037: the kridi balance once this ticket is on the account
    if let (Some(_), Some(balance)) = (&r.customer, r.customer_balance) {
        rows.push(strong_pair(
            w.key("receipt.customerBalance"),
            amount(balance, lang),
        ));
    }
    if r.discount_total.is_positive() {
        rows.push(pair(
            w.get(|l| &l.you_saved),
            amount(r.discount_total, lang),
        ));
    }
    // LOY-002: the points this ticket earned and the card's balance after it
    if let Some(loyalty) = &r.loyalty {
        rows.push(text(loyalty.line(lang), Align::Start));
    }
    rows.push(Row::Rule);
    if r.total_due != Money::ZERO {
        rows.push(text(
            format!("{} : {}", w.get(|l| &l.in_letters), r.total_in_letters),
            Align::Start,
        ));
    }
    if r.ifu_mention {
        rows.push(text(w.get(|l| &l.ifu_mention), Align::Center));
    }
    if let Some(note) = &r.note {
        rows.push(text(note.clone(), Align::Start));
    }
    if let Some(footer) = &r.footer_text {
        rows.push(text(footer.clone(), Align::Center));
    }
    // M7: the linked DZBuild store, when the owner prints it ("Commandez en ligne")
    if let Some(url) = r.store_url.as_deref()
        && let Some(matrix) = pos_docs::a4::qr::qr(url)
    {
        rows.push(Row::Rule);
        rows.push(strong(w.key("receipt.orderOnline"), Align::Center));
        rows.push(Row::Qr {
            size: matrix.size,
            modules: matrix.modules,
        });
        rows.push(text(
            url.trim_start_matches("https://").trim_end_matches('/'),
            Align::Center,
        ));
    }
    rows.push(strong(w.get(|l| &l.thanks), Align::Center));
    rows.push(Row::Feed { lines: 1 });
    rows.push(Row::Barcode {
        data: r.barcode.clone(),
    });
    Layout { rtl: r.rtl, rows }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::escpos::dump;
    use crate::golden::assert_golden;
    use crate::profile::{PaperWidth, Preset, Profile, RenderMode};
    use crate::raster::{Typesetter, logo_bitmap};
    use crate::render::{DrawerKick, Job, preview, render};
    use crate::testdata::{cash, logo_png, ticket};
    use pos_docs::receipt::{ReceiptCustomer, ReceiptPayment};
    use sha2::{Digest, Sha256};

    fn print(
        receipt: &Receipt,
        second: Option<Lang>,
        profile: &Profile,
    ) -> crate::render::Rendered {
        let layout = receipt_layout(receipt, second);
        render(
            &Job {
                layout: &layout,
                logo: None,
                drawer: DrawerKick::None,
            },
            profile,
            &mut Typesetter::new(),
        )
        .unwrap()
    }

    fn digest(bytes: &[u8]) -> String {
        format!(
            "{} bytes, sha256 {}",
            bytes.len(),
            hex::encode(Sha256::digest(bytes))
        )
    }

    /// The bytes outside the `GS v 0` bands: the commands and any text-mode characters (the
    /// dots inside a band are pixels, where 0x3F is as likely as any byte).
    fn commands(bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i..].starts_with(&[0x1D, b'v', b'0']) && i + 8 <= bytes.len() {
                let row = usize::from(bytes[i + 4]) + 256 * usize::from(bytes[i + 5]);
                let rows = usize::from(bytes[i + 6]) + 256 * usize::from(bytes[i + 7]);
                i += 8 + row * rows;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        out
    }

    #[test]
    fn ticket_layout_fr() {
        insta::assert_debug_snapshot!(receipt_layout(&ticket(Lang::Fr), None));
    }

    #[test]
    fn ticket_text_fr_80() {
        let out = print(
            &ticket(Lang::Fr),
            None,
            &Profile::preset(Preset::Generic, PaperWidth::Mm80),
        );
        assert!(!out.raster, "a French ticket prints in text mode");
        insta::assert_snapshot!(dump(&out.bytes));
    }

    #[test]
    fn ticket_text_en_58() {
        let out = print(
            &ticket(Lang::En),
            None,
            &Profile::preset(Preset::Xprinter, PaperWidth::Mm58),
        );
        assert!(!out.raster);
        assert!(
            out.bytes.starts_with(&[0x1B, b'@', 0x1C, b'.']),
            "Xprinter leaves Chinese mode right after the reset"
        );
        insta::assert_snapshot!(dump(&out.bytes));
    }

    #[test]
    fn ticket_ar_80() {
        let receipt = ticket(Lang::Ar);
        let profile = Profile::preset(Preset::Generic, PaperWidth::Mm80);
        let out = print(&receipt, None, &profile);
        assert!(out.raster, "Arabic always prints as an image");
        assert!(out.bytes.windows(3).any(|w| w == [0x1D, b'v', b'0']));
        assert!(
            !commands(&out.bytes)
                .windows(3)
                .any(|w| w == [0x1B, b't', 19]),
            "no code page for Arabic"
        );
        insta::assert_snapshot!(digest(&out.bytes));
        let layout = receipt_layout(&receipt, None);
        let image = preview(
            &Job {
                layout: &layout,
                logo: None,
                drawer: DrawerKick::None,
            },
            &profile,
            &mut Typesetter::new(),
        );
        assert_golden("ticket_ar_80", &image);
    }

    #[test]
    fn ticket_bilingual_58() {
        let receipt = ticket(Lang::Fr);
        let profile = Profile::preset(Preset::Generic, PaperWidth::Mm58);
        let layout = receipt_layout(&receipt, Some(Lang::Ar));
        let texts: Vec<&str> = layout.texts().collect();
        for wanted in [
            "Net à payer / الصافي للدفع",
            "NIF / رقم التعريف الجبائي 001216098765432",
            "TVA / الرسم على القيمة المضافة 19 % (518,91)",
        ] {
            assert!(texts.iter().any(|t| t.contains(wanted)), "{wanted}");
        }
        let out = render(
            &Job {
                layout: &layout,
                logo: None,
                drawer: DrawerKick::None,
            },
            &profile,
            &mut Typesetter::new(),
        )
        .unwrap();
        assert!(out.raster);
        insta::assert_snapshot!(digest(&out.bytes));
        let image = preview(
            &Job {
                layout: &layout,
                logo: None,
                drawer: DrawerKick::None,
            },
            &profile,
            &mut Typesetter::new(),
        );
        assert_golden("ticket_bilingual_58", &image);
    }

    /// UX-042 / DOC-013: a bilingual receipt pairs every label, legal mentions included; only
    /// the owner's texts, the figures and the amount in letters stay in the print language.
    #[test]
    fn bilingual_receipts_pair_every_label() {
        let mut receipt = ticket(Lang::Fr);
        receipt.doc_type = DocType::ReturnNote;
        receipt.original_number = Some("TK2026-C1-000123".into());
        receipt.header.nis = Some("000216001234567".into());
        receipt.header.ai = Some("16012345678".into());
        receipt.stamp_absorbed = Money(1_100);
        receipt.payments = vec![ReceiptPayment {
            method: PaymentMethod::Cheque,
            amount: Money(107_850),
            reference: Some("0012345".into()),
            cheque_bank: Some("BNA".into()),
            cheque_date: Some("2026-09-28".into()),
        }];
        let texts: Vec<String> = receipt_layout(&receipt, Some(Lang::Ar))
            .texts()
            .map(str::to_owned)
            .collect();
        for wanted in [
            "Tél. / الهاتف 0550 12 34 56",
            "RC / السجل التجاري 16/00-1234567 B 22",
            "NIF / رقم التعريف الجبائي 001216098765432",
            "NIS / رقم التعريف الإحصائي 000216001234567",
            "AI / رقم المادة الضريبية 16012345678",
            "N° / رقم TK2026-C1-000123",
            "Date / التاريخ 28/09/2026 12:00",
            "Caisse / الصندوق C1",
            "Caissier / أمين الصندوق Yacine",
            "Ticket d'origine / التذكرة الأصلية TK2026-C1-000123",
            "TVA / الرسم على القيمة المضافة 9 % (412,84)",
            "Droit de timbre pris en charge par le magasin / حقوق الطابع على عاتق المحل : 11,00",
            "  Banque / البنك BNA",
            "  Date du chèque / تاريخ الصك 28/09/2026",
            "Arrêté à la somme de / أوقف هذا المبلغ على : mille soixante-dix-huit dinars et cinquante centimes",
        ] {
            assert!(texts.iter().any(|t| t == wanted), "{wanted}\n{texts:#?}");
        }
    }

    #[test]
    fn an_arabic_product_name_switches_a_french_ticket_to_raster() {
        let mut receipt = ticket(Lang::Fr);
        receipt.lines[0].description = "حليب كانديا".into();
        let out = print(
            &receipt,
            None,
            &Profile::preset(Preset::Generic, PaperWidth::Mm80),
        );
        assert!(out.raster);
    }

    #[test]
    fn forced_text_mode_never_prints_arabic() {
        let profile = Profile {
            render: RenderMode::Text,
            ..Profile::preset(Preset::Generic, PaperWidth::Mm80)
        };
        let out = print(&ticket(Lang::Ar), None, &profile);
        assert!(
            out.raster,
            "text mode is refused rather than printing question marks"
        );
        let commands = commands(&out.bytes);
        assert!(!commands.contains(&b'?'), "{}", dump(&commands));
        assert!(
            !commands.windows(2).any(|w| w == [0x1B, b't']),
            "no code page selected, no text sent"
        );
    }

    #[test]
    fn a_logo_heads_the_ticket() {
        let profile = Profile::preset(Preset::Generic, PaperWidth::Mm80);
        let logo = logo_bitmap(&logo_png(), 345, 160).unwrap();
        let layout = receipt_layout(&ticket(Lang::Fr), None);
        let job = Job {
            layout: &layout,
            logo: Some(&logo),
            drawer: DrawerKick::None,
        };
        let out = render(&job, &profile, &mut Typesetter::new()).unwrap();
        assert!(
            !out.raster,
            "the logo alone keeps a Latin ticket in text mode"
        );
        let raster_at = out
            .bytes
            .windows(3)
            .position(|w| w == [0x1D, b'v', b'0'])
            .unwrap();
        let name_at = out
            .bytes
            .windows(9)
            .position(|w| w == b"Sup\x82rette")
            .unwrap();
        assert!(raster_at < name_at, "logo first");
        assert_golden(
            "ticket_fr_logo_80",
            &preview(&job, &profile, &mut Typesetter::new()),
        );
    }

    #[test]
    fn a_return_note_prints_the_refund_and_its_ticket() {
        let mut receipt = ticket(Lang::Fr);
        receipt.doc_type = DocType::ReturnNote;
        receipt.number = "BR2026-C1-000004".into();
        receipt.original_number = Some("TK2026-C1-000123".into());
        receipt.total_due = Money(-30_000);
        receipt.payments = vec![cash(-30_000)];
        receipt.change = Money(0);
        let texts: Vec<String> = receipt_layout(&receipt, None)
            .texts()
            .map(str::to_owned)
            .collect();
        assert!(texts.iter().any(|t| t == "Bon de retour"));
        assert!(
            texts
                .iter()
                .any(|t| t.contains("Ticket d'origine TK2026-C1-000123"))
        );
        assert!(texts.iter().any(|t| t == "Remboursement"));
        assert!(texts.iter().any(|t| t == "-300,00 DA"));
    }

    /// DOC-015, SAL-037: a ticket sold to a known customer names him under the cashier (name and
    /// code, never his phone nor his address: loi 18-07) and gives his account balance after it.
    #[test]
    fn a_ticket_names_its_customer_and_his_balance_after_it() {
        let karim = |lang: Lang| {
            ticket(lang).with_customer(
                ReceiptCustomer {
                    name: "Karim Benali".into(),
                    code: "C00001".into(),
                    phone: Some("0550123456".into()),
                    address: Some("Bab Ezzouar".into()),
                },
                Some(Money(300_000)),
            )
        };
        let layout = receipt_layout(&karim(Lang::Fr), None);
        let texts: Vec<&str> = layout.texts().collect();
        let at = |wanted: &str| texts.iter().position(|t| *t == wanted);
        let cashier = at("Caissier Yacine").unwrap();
        assert_eq!(at("Client Karim Benali"), Some(cashier + 1), "{texts:#?}");
        assert_eq!(at("Code client C00001"), Some(cashier + 2), "{texts:#?}");
        let balance = at("Solde de votre compte").unwrap();
        assert_eq!(texts[balance + 1], "3 000,00");
        assert!(balance > at("Monnaie rendue").unwrap(), "after the payment");
        assert!(
            !texts
                .iter()
                .any(|t| t.contains("0550123456") || t.contains("Bab Ezzouar")),
            "{texts:#?}"
        );
        let bilingual: Vec<String> = receipt_layout(&karim(Lang::Fr), Some(Lang::Ar))
            .texts()
            .map(str::to_owned)
            .collect();
        for wanted in [
            "Client / الزبون Karim Benali",
            "Code client / رمز الزبون C00001",
            "Solde de votre compte / رصيد حسابكم",
        ] {
            assert!(bilingual.iter().any(|t| t == wanted), "{wanted}");
        }
        let out = print(
            &karim(Lang::Fr),
            None,
            &Profile::preset(Preset::Generic, PaperWidth::Mm80),
        );
        assert!(!out.raster);
        insta::assert_snapshot!(dump(&out.bytes));
        // no customer, no block: the anonymous ticket is unchanged
        let anonymous: Vec<String> = receipt_layout(&ticket(Lang::Fr), None)
            .texts()
            .map(str::to_owned)
            .collect();
        assert!(!anonymous.iter().any(|t| t.starts_with("Client")
            || t.starts_with("Code client")
            || t == "Solde de votre compte"));
        // a customer who used no account tender has no balance line
        let mut no_balance = karim(Lang::Fr);
        no_balance.customer_balance = None;
        let texts: Vec<String> = receipt_layout(&no_balance, None)
            .texts()
            .map(str::to_owned)
            .collect();
        assert!(texts.iter().any(|t| t == "Client Karim Benali"));
        assert!(!texts.iter().any(|t| t == "Solde de votre compte"));
    }

    #[test]
    fn a_reprint_says_duplicata() {
        let mut receipt = ticket(Lang::Fr);
        receipt.duplicate = true;
        let layout = receipt_layout(&receipt, None);
        assert!(
            layout
                .rows
                .iter()
                .any(|r| matches!(r, Row::Text { text, large: true, .. } if text == "DUPLICATA"))
        );
    }

    /// Finding 39: every ticket of the demo shop says it has no fiscal value, in its language.
    #[test]
    fn a_demo_ticket_says_it_has_no_fiscal_value() {
        for (lang, mark) in [
            (Lang::Fr, "DÉMO – sans valeur fiscale"),
            (Lang::Ar, "تجريبي – بدون قيمة جبائية"),
            (Lang::En, "DEMO – no fiscal value"),
        ] {
            let mut receipt = ticket(lang);
            assert!(!receipt_layout(&receipt, None).texts().any(|t| t == mark));
            receipt.header.demo = true;
            let layout = receipt_layout(&receipt, None);
            assert!(
                layout
                    .rows
                    .iter()
                    .any(|r| matches!(r, Row::Text { text, large: true, .. } if text == mark)),
                "{lang:?}"
            );
        }
    }

    #[test]
    fn ifu_tickets_show_no_vat() {
        let mut receipt = ticket(Lang::Fr);
        receipt.show_tax = false;
        receipt.ifu_mention = true;
        receipt.taxes.clear();
        let texts: Vec<String> = receipt_layout(&receipt, None)
            .texts()
            .map(str::to_owned)
            .collect();
        assert!(!texts.iter().any(|t| t.starts_with("TVA")));
        assert!(texts.iter().any(|t| t == "Régime IFU – TVA non applicable"));
    }

    /// PAY-023: the duty exemption of a cheque rests on its number, bank and date on the receipt.
    #[test]
    fn a_cheque_prints_its_number_bank_and_date() {
        let mut receipt = ticket(Lang::Fr);
        receipt.stamp_duty = Money::ZERO;
        receipt.total_due = Money(106_750);
        receipt.change = Money::ZERO;
        receipt.payments = vec![ReceiptPayment {
            method: PaymentMethod::Cheque,
            amount: Money(106_750),
            reference: Some("0012345".into()),
            cheque_bank: Some("BNA".into()),
            cheque_date: Some("2026-09-28".into()),
        }];
        let texts: Vec<String> = receipt_layout(&receipt, None)
            .texts()
            .map(str::to_owned)
            .collect();
        let at = |wanted: &str| texts.iter().position(|t| t == wanted);
        let cheque = at("Chèque").unwrap();
        assert_eq!(at("  0012345"), Some(cheque + 2), "the amount sits between");
        assert_eq!(at("  Banque BNA"), Some(cheque + 3));
        assert_eq!(at("  Date du chèque 28/09/2026"), Some(cheque + 4));
        let out = print(
            &receipt,
            None,
            &Profile::preset(Preset::Generic, PaperWidth::Mm80),
        );
        assert!(!out.raster);
        assert!(
            out.bytes
                .windows(28)
                .any(|w| w == b"  Date du ch\x8Aque 28/09/2026\n"),
            "{}",
            dump(&out.bytes)
        );
    }

    /// Owner texts keep their line breaks and stay in text mode: the code page check runs on
    /// the printed lines, never on a line feed.
    #[test]
    fn a_multi_line_footer_keeps_a_latin_ticket_in_text_mode() {
        let mut receipt = ticket(Lang::Fr);
        receipt.footer_text = Some("Ouvert 7j/7\nde 7h à 22h".into());
        receipt.header.header_text = Some("Livraison\tgratuite".into());
        let out = print(
            &receipt,
            None,
            &Profile::preset(Preset::Generic, PaperWidth::Mm80),
        );
        assert!(!out.raster, "{}", dump(&out.bytes));
        let text = dump(&out.bytes);
        assert!(
            text.contains("Ouvert 7j/7<0A>\nde 7h <85> 22h<0A>"),
            "{text}"
        );
        assert!(text.contains("Livraison gratuite<0A>"), "{text}");
    }

    #[test]
    fn a_receipt_with_the_store_url_ends_with_the_order_online_qr() {
        let mut r = crate::testdata::ticket(Lang::Fr);
        r.store_url = Some("https://amel.dzbuild.app".into());
        let layout = receipt_layout(&r, None);
        let qr_at = layout
            .rows
            .iter()
            .position(|row| matches!(row, Row::Qr { .. }))
            .expect("a QR row");
        assert!(
            layout.rows[..qr_at]
                .iter()
                .any(|row| matches!(row, Row::Text { text, .. } if text == "Commandez en ligne"))
        );
        assert!(
            layout.rows[qr_at..]
                .iter()
                .any(|row| matches!(row, Row::Text { text, .. } if text == "amel.dzbuild.app"))
        );
        r.store_url = None;
        assert!(
            !receipt_layout(&r, None)
                .rows
                .iter()
                .any(|row| matches!(row, Row::Qr { .. }))
        );
    }
}
