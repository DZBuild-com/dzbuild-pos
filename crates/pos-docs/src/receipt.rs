//! The receipt model (spec §9.1; DOC-005/014/015/016/018): what a 58/80 mm ticket or return
//! note shows, built from a validated document and the shop's identity. Amounts and codes only;
//! the words around them come from [`labels`] in the print language, and `pos-hw` (M1b) turns
//! both into ESC/POS bytes or an Arabic raster.

use crate::letters::amount_in_letters;
use crate::locales;
use pos_core::company::{Company, TaxRegime};
use pos_core::documents::{DocStatus, DocType, Document};
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty, Rate};
use pos_core::payments::PaymentMethod;
use pos_core::tax::timbre::StampMode;
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptHeader {
    /// The shop's name in the print language (fallback fr → ar → en).
    pub name: String,
    pub legal_form: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub rc: Option<String>,
    pub nif: Option<String>,
    pub nis: Option<String>,
    pub ai: Option<String>,
    /// The owner's header text in the print language.
    pub header_text: Option<String>,
    /// Built from the demo shop's database: "DÉMO – sans valeur fiscale" is printed under the
    /// title (finding 39).
    pub demo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptComponent {
    pub description: String,
    pub qty: Qty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptLine {
    pub description: String,
    pub qty: Qty,
    pub unit_code: String,
    pub unit_price: Money,
    /// Discount given on the line (positive on a sale).
    pub discount: Money,
    /// TTC of the line.
    pub total: Money,
    pub tax_rate_bp: Rate,
    pub note: Option<String>,
    /// What a kit contained.
    pub components: Vec<ReceiptComponent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptTax {
    pub rate_bp: Rate,
    pub base_ht: Money,
    pub tax: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptPayment {
    pub method: PaymentMethod,
    pub amount: Money,
    /// Card transaction, cheque number or transfer reference.
    pub reference: Option<String>,
    /// A cheque's bank and date (`YYYY-MM-DD`): printed, they justify its duty exemption (PAY-023).
    pub cheque_bank: Option<String>,
    pub cheque_date: Option<String>,
}

/// The customer of a ticket (DOC-015): printed under the cashier line with the account balance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptCustomer {
    pub name: String,
    pub code: String,
    /// Never printed on a thermal ticket (name and code only, loi 18-07).
    pub phone: Option<String>,
    pub address: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub lang: Lang,
    pub rtl: bool,
    pub doc_type: DocType,
    pub number: String,
    /// `dd/mm/yyyy HH:MM`, Algiers time of the validation.
    pub local_time: String,
    pub cashier: String,
    pub terminal_code: String,
    pub header: ReceiptHeader,
    pub lines: Vec<ReceiptLine>,
    pub item_count: Qty,
    /// Régime réel: HT, TVA per rate and TTC are printed. IFU: TTC only, with the mention.
    pub show_tax: bool,
    pub ifu_mention: bool,
    pub subtotal_ht: Money,
    pub taxes: Vec<ReceiptTax>,
    /// "Vous avez économisé" (PRM-006).
    pub discount_total: Money,
    pub total_ttc: Money,
    /// The duty added to the amount due.
    pub stamp_duty: Money,
    /// The duty the shop paid itself (printed as a note, not added).
    pub stamp_absorbed: Money,
    pub rounding: Money,
    /// "Net à payer" (negative on a return note: the refund).
    pub total_due: Money,
    /// Tenders (change excluded).
    pub payments: Vec<ReceiptPayment>,
    pub change: Money,
    pub total_in_letters: String,
    /// The ticket a return note refers to.
    pub original_number: Option<String>,
    pub note: Option<String>,
    pub footer_text: Option<String>,
    /// Reprint: marked DUPLICATA (DOC-018).
    pub duplicate: bool,
    /// Printed as a Code 128 barcode for returns (DOC-016).
    pub barcode: String,
    /// "Commandez en ligne": the linked DZBuild store (M7), printed with its QR.
    pub store_url: Option<String>,
    /// The customer's kridi balance after the sale.
    pub customer_balance: Option<Money>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub customer: Option<ReceiptCustomer>,
    /// Restaurant tickets (RST-009): the takeaway number printed large, or the table.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub dining: Option<ReceiptDining>,
    /// The loyalty points the ticket earned (LOY-002), printed under the totals.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub loyalty: Option<ReceiptLoyalty>,
}

/// The points line of a ticket that earned points: the command layer fills it from
/// `loyalty::account` after the sale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptLoyalty {
    pub earned: i64,
    pub balance: i64,
    pub card: String,
}

impl ReceiptLoyalty {
    /// "Points fidélité : +23 — solde 36" in the receipt's language.
    pub fn line(&self, lang: Lang) -> String {
        label(lang, "receipt.loyalty.line")
            .replace("{earned}", &self.earned.to_string())
            .replace("{balance}", &self.balance.to_string())
    }
}

/// The dining block of a restaurant ticket, in the receipt's language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptDining {
    /// "Sur place", "À emporter" or "Livraison" in the receipt's language.
    pub mode: String,
    pub order_no: Option<i64>,
    /// "Table T4" or the ticket's label.
    pub place: Option<String>,
    pub covers: u8,
}

/// The dining block of `info` in `lang` (set on the receipt by the print path).
pub fn dining(info: &pos_core::restaurant::bill::DiningInfo, lang: Lang) -> ReceiptDining {
    let key = format!("receipt.dining.{}", info.mode.as_str());
    ReceiptDining {
        mode: label(lang, &key),
        order_no: info.order_no,
        place: info
            .table_code
            .as_ref()
            .map(|t| label(lang, "receipt.dining.table").replace("{table}", t))
            .or_else(|| info.label.clone()),
        covers: info.covers,
    }
}

impl Receipt {
    /// Adds the ticket's customer and his balance after the sale (M2).
    pub fn with_customer(mut self, customer: ReceiptCustomer, balance: Option<Money>) -> Receipt {
        self.customer = Some(customer);
        self.customer_balance = balance;
        self
    }

    /// Adds the points the ticket earned and the balance after it (LOY-002).
    pub fn with_loyalty(mut self, loyalty: ReceiptLoyalty) -> Receipt {
        self.loyalty = Some(loyalty);
        self
    }
}

/// Everything a receipt needs besides the document.
#[derive(Debug, Clone, Copy)]
pub struct ReceiptSource<'a> {
    pub document: &'a Document,
    pub company: &'a Company,
    pub cashier_name: &'a str,
    pub terminal_code: &'a str,
    pub original_number: Option<&'a str>,
    pub duplicate: bool,
}

fn in_lang<'a>(lang: Lang, ar: &'a str, fr: &'a str, en: &'a str) -> &'a str {
    let preferred = match lang {
        Lang::Ar => ar,
        Lang::Fr => fr,
        Lang::En => en,
    };
    [preferred, fr, ar, en]
        .into_iter()
        .find(|s| !s.trim().is_empty())
        .unwrap_or("")
}

fn in_lang_opt(
    lang: Lang,
    ar: &Option<String>,
    fr: &Option<String>,
    en: &Option<String>,
) -> Option<String> {
    let text = in_lang(
        lang,
        ar.as_deref().unwrap_or(""),
        fr.as_deref().unwrap_or(""),
        en.as_deref().unwrap_or(""),
    );
    (!text.is_empty()).then(|| text.to_owned())
}

/// The shop's identity at the top of a receipt, in `lang` (also the versement receipt and the
/// statement of [`crate::settlement`]).
pub fn receipt_header(company: &Company, lang: Lang) -> ReceiptHeader {
    ReceiptHeader {
        name: in_lang(lang, &company.name_ar, &company.name_fr, &company.name_en).to_owned(),
        legal_form: company.legal_form.clone(),
        address: company.address.clone(),
        phone: company.phone.clone(),
        rc: company.rc.clone(),
        nif: company.nif.clone(),
        nis: company.nis.clone(),
        ai: company.ai.clone(),
        header_text: in_lang_opt(
            lang,
            &company.receipt_header_ar,
            &company.receipt_header_fr,
            &company.receipt_header_en,
        ),
        demo: company.demo,
    }
}

/// The receipt of a validated document, in the document's print language.
pub fn build(src: &ReceiptSource<'_>) -> Result<Receipt, AppError> {
    let doc = src.document;
    let company = src.company;
    let number = doc
        .number
        .clone()
        .filter(|_| doc.status == DocStatus::Validated)
        .ok_or_else(|| AppError::conflict("document_not_validated"))?;
    let lang = doc.lang;
    let mut lines: Vec<ReceiptLine> = Vec::new();
    let mut ids: Vec<&str> = Vec::new();
    for l in &doc.lines {
        if let Some(parent) = &l.parent_line_id {
            if let Some(index) = ids.iter().position(|id| id == parent) {
                lines[index].components.push(ReceiptComponent {
                    description: l.description.clone(),
                    qty: l.qty,
                });
            }
            continue;
        }
        ids.push(&l.id);
        lines.push(ReceiptLine {
            description: l.description.clone(),
            qty: l.qty,
            unit_code: l.unit_code.clone(),
            unit_price: l.unit_price,
            discount: l.discount_amount,
            total: l.total_ttc,
            tax_rate_bp: l.tax_rate_bp,
            note: l.note.clone(),
            components: Vec::new(),
        });
    }
    let item_count = lines
        .iter()
        .try_fold(Qty::ZERO, |acc, l| acc.checked_add(l.qty))
        .unwrap_or(Qty::ZERO);
    let reel = doc.tax_regime == TaxRegime::Reel;
    let local_time = doc
        .validated_at
        .map(|ms| {
            pos_core::time::algiers_datetime(ms)
                .format("%d/%m/%Y %H:%M")
                .to_string()
        })
        .unwrap_or_default();
    Ok(Receipt {
        lang,
        rtl: lang.is_rtl(),
        doc_type: doc.doc_type,
        number: number.clone(),
        local_time,
        cashier: src.cashier_name.to_owned(),
        terminal_code: src.terminal_code.to_owned(),
        header: receipt_header(company, lang),
        lines,
        item_count,
        show_tax: reel,
        ifu_mention: !reel,
        subtotal_ht: doc.subtotal_ht,
        taxes: if reel {
            doc.taxes
                .iter()
                .map(|t| ReceiptTax {
                    rate_bp: t.rate_bp,
                    base_ht: t.base_ht,
                    tax: t.tax_amount,
                })
                .collect()
        } else {
            Vec::new()
        },
        discount_total: doc.discount_total,
        total_ttc: doc.total_ttc,
        stamp_duty: if doc.stamp_mode == StampMode::ChargeCustomer {
            doc.stamp_duty
        } else {
            Money::ZERO
        },
        stamp_absorbed: if doc.stamp_mode == StampMode::Absorb {
            doc.stamp_duty
        } else {
            Money::ZERO
        },
        rounding: doc.rounding,
        total_due: doc.total_due,
        payments: doc
            .payments
            .iter()
            .filter(|p| p.method != PaymentMethod::Change)
            .map(|p| ReceiptPayment {
                method: p.method,
                amount: p.amount,
                reference: p.reference.clone(),
                cheque_bank: p.cheque_bank.clone(),
                cheque_date: p.cheque_date.clone(),
            })
            .collect(),
        change: doc.change_given,
        total_in_letters: amount_in_letters(lang, doc.total_due),
        original_number: src.original_number.map(str::to_owned),
        note: doc.note.clone(),
        footer_text: in_lang_opt(
            lang,
            &company.receipt_footer_ar,
            &company.receipt_footer_fr,
            &company.receipt_footer_en,
        ),
        duplicate: src.duplicate,
        barcode: number,
        store_url: None,
        customer_balance: None,
        customer: None,
        dining: None,
        loyalty: None,
    })
}

/// The words printed around a receipt's figures, in one language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptLabels {
    pub title: String,
    pub duplicate: String,
    pub number: String,
    pub date: String,
    pub cashier: String,
    pub terminal: String,
    pub qty: String,
    pub unit_price: String,
    pub amount: String,
    pub items: String,
    pub subtotal_ht: String,
    pub vat: String,
    pub total_ttc: String,
    pub stamp_duty: String,
    pub stamp_absorbed: String,
    pub rounding: String,
    pub net_to_pay: String,
    pub paid: String,
    pub change: String,
    pub cheque_bank: String,
    pub cheque_date: String,
    pub you_saved: String,
    pub ifu_mention: String,
    pub refund: String,
    pub original: String,
    pub in_letters: String,
    pub thanks: String,
    pub rc: String,
    pub nif: String,
    pub nis: String,
    pub ai: String,
    pub phone: String,
    /// A line's discount.
    pub discount: String,
    /// The ticket's customer, by name and code only (DOC-015), and his balance after it.
    pub customer: String,
    pub customer_code: String,
    pub customer_balance: String,
}

/// A document word; the key itself if a catalogue lacks it (the parity tests keep them full).
pub fn label(lang: Lang, key: &str) -> String {
    locales::text(lang.as_str(), key).unwrap_or_else(|| key.to_owned())
}

pub fn method_label(lang: Lang, method: PaymentMethod) -> String {
    label(lang, &format!("receipt.methods.{}", method.as_str()))
}

/// The labels of a ticket (`doc.ticket`) or a return note (`doc.returnNote`) in `lang`.
pub fn labels(lang: Lang, doc_type: DocType) -> ReceiptLabels {
    let l = |key: &str| label(lang, key);
    ReceiptLabels {
        title: l(match doc_type {
            DocType::ReturnNote => "doc.returnNote",
            DocType::Invoice => "doc.invoice",
            DocType::CreditNote => "doc.creditNote",
            _ => "doc.ticket",
        }),
        duplicate: l("receipt.duplicate"),
        number: l("receipt.number"),
        date: l("receipt.date"),
        cashier: l("receipt.cashier"),
        terminal: l("receipt.terminal"),
        qty: l("receipt.qty"),
        unit_price: l("receipt.unitPrice"),
        amount: l("receipt.amount"),
        items: l("receipt.items"),
        subtotal_ht: l("receipt.subtotalHt"),
        vat: l("receipt.vat"),
        total_ttc: l("receipt.totalTtc"),
        stamp_duty: l("receipt.stampDuty"),
        stamp_absorbed: l("receipt.stampAbsorbed"),
        rounding: l("receipt.rounding"),
        net_to_pay: l("receipt.netToPay"),
        paid: l("receipt.paid"),
        change: l("receipt.change"),
        cheque_bank: l("receipt.chequeBank"),
        cheque_date: l("receipt.chequeDate"),
        you_saved: l("receipt.youSaved"),
        ifu_mention: l("receipt.ifuMention"),
        refund: l("receipt.refund"),
        original: l("receipt.original"),
        in_letters: l("receipt.inLetters"),
        thanks: l("receipt.thanks"),
        rc: l("receipt.rc"),
        nif: l("receipt.nif"),
        nis: l("receipt.nis"),
        ai: l("receipt.ai"),
        phone: l("receipt.phone"),
        discount: l("receipt.discount"),
        customer: l("receipt.customer"),
        customer_code: l("receipt.customerCode"),
        customer_balance: l("receipt.customerBalance"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use pos_core::company::PriceMode;
    use pos_core::documents::{DocumentLine, DocumentTax, Payment};

    fn line(
        id: &str,
        parent: Option<&str>,
        description: &str,
        qty: i64,
        unit_price: i64,
        total: i64,
        discount: i64,
    ) -> DocumentLine {
        DocumentLine {
            id: id.into(),
            line_no: 0,
            product_id: Some(format!("p-{id}")),
            description: description.into(),
            description_alt: None,
            qty: Qty(qty),
            unit_code: "pc".into(),
            unit_price: Money(unit_price),
            price_overridden: false,
            discount_bp: Rate::ZERO,
            discount_amount: Money(discount),
            tax_rate_bp: Rate(1_900),
            total_ht: Money(0),
            tax_amount: Money(0),
            total_ttc: Money(total),
            cost_price: Money::ZERO,
            lot_id: None,
            serial_no: None,
            parent_line_id: parent.map(str::to_owned),
            origin_line_id: None,
            restock_location_id: None,
            reason_code: None,
            note: None,
        }
    }

    fn payment(method: PaymentMethod, amount: i64) -> Payment {
        Payment {
            id: "pay".into(),
            method,
            amount: Money(amount),
            reference: None,
            cheque_bank: None,
            cheque_date: None,
            cash_session_id: None,
        }
    }

    /// 2 × Lait at 450.00 with 50.00 off, one school kit (2 pens inside); cash 1 500.00.
    fn ticket(lang: Lang, regime: TaxRegime) -> Document {
        Document {
            id: "doc-1".into(),
            doc_type: DocType::Ticket,
            number: Some("TK2026-C1-000123".into()),
            series: "C1".into(),
            fiscal_year: Some(2026),
            status: DocStatus::Validated,
            business_date: "2026-09-28".into(),
            customer_id: None,
            supplier_id: None,
            user_id: "u1".into(),
            terminal_id: "t1".into(),
            cash_session_id: None,
            price_level_id: None,
            lang,
            tax_regime: regime,
            price_mode: PriceMode::Ttc,
            subtotal_ht: Money(100_840),
            discount_total: Money(5_000),
            tax_total: Money(19_160),
            total_ttc: Money(120_000),
            stamp_duty: Money(1_200),
            stamp_mode: StampMode::ChargeCustomer,
            rounding: Money::ZERO,
            total_due: Money(121_200),
            paid_total: Money(150_000),
            change_given: Money(28_800),
            credit_amount: Money::ZERO,
            note: None,
            parent_id: None,
            source_ref: None,
            validated_at: Some(1_790_593_200_000),
            chain_seq: Some(123),
            prev_hash: None,
            hash: None,
            created_at: 0,
            valid_until: None,
            due_date: None,
            second_lang: None,
            credit_kind: None,
            origin: None,
            parties: Vec::new(),
            lines: vec![
                line("l1", None, "Lait Candia 1 L", 2_000, 45_000, 85_000, 5_000),
                line("l2", None, "Kit scolaire", 1_000, 35_000, 35_000, 0),
                line("l3", Some("l2"), "Stylo", 2_000, 0, 0, 0),
            ],
            taxes: vec![DocumentTax {
                rate_bp: Rate(1_900),
                base_ht: Money(100_840),
                tax_amount: Money(19_160),
            }],
            payments: vec![
                payment(PaymentMethod::Cash, 150_000),
                payment(PaymentMethod::Change, -28_800),
            ],
        }
    }

    fn company() -> Company {
        Company {
            name_ar: "مغازة الأمل".into(),
            name_fr: "Supérette El Amel".into(),
            name_en: String::new(),
            legal_form: Some("EURL".into()),
            capital: None,
            address: Some("12 rue Larbi Ben M'hidi, Alger".into()),
            wilaya: Some("16".into()),
            phone: Some("0555 12 34 56".into()),
            email: None,
            rc: Some("16/00-1234567B26".into()),
            nif: Some("000116001234567".into()),
            nis: Some("000116010012345".into()),
            ai: Some("16012345678".into()),
            rib: None,
            tax_regime: TaxRegime::Reel,
            price_mode: PriceMode::Ttc,
            receipt_header_ar: None,
            receipt_header_fr: Some("Ouvert 7j/7".into()),
            receipt_header_en: None,
            receipt_footer_ar: Some("شكرا".into()),
            receipt_footer_fr: None,
            receipt_footer_en: None,
            default_doc_lang: Lang::Fr,
            has_logo: false,
            updated_at: 0,
            demo: false,
        }
    }

    fn source<'a>(doc: &'a Document, company: &'a Company, duplicate: bool) -> ReceiptSource<'a> {
        ReceiptSource {
            document: doc,
            company,
            cashier_name: "Yacine",
            terminal_code: "C1",
            original_number: None,
            duplicate,
        }
    }

    #[test]
    fn french_ticket_model() {
        let (doc, company) = (ticket(Lang::Fr, TaxRegime::Reel), company());
        let receipt = build(&source(&doc, &company, false)).unwrap();
        insta::assert_json_snapshot!("ticket_fr", receipt);
    }

    #[test]
    fn kits_group_their_components_and_change_is_not_a_tender() {
        let (doc, company) = (ticket(Lang::Fr, TaxRegime::Reel), company());
        let r = build(&source(&doc, &company, true)).unwrap();
        assert_eq!(r.lines.len(), 2);
        assert_eq!(
            r.lines[1].components,
            [ReceiptComponent {
                description: "Stylo".into(),
                qty: Qty(2_000)
            }]
        );
        assert_eq!(
            r.payments,
            [ReceiptPayment {
                method: PaymentMethod::Cash,
                amount: Money(150_000),
                reference: None,
                cheque_bank: None,
                cheque_date: None,
            }]
        );
        assert_eq!(
            (r.change, r.item_count, r.duplicate, r.barcode.as_str()),
            (Money(28_800), Qty(3_000), true, "TK2026-C1-000123")
        );
        assert_eq!(r.local_time, "28/09/2026 12:00");
        assert_eq!(r.total_in_letters, "mille deux cent douze dinars");
    }

    /// PAY-023: a cheque is exempt from the duty only with its details on the receipt.
    #[test]
    fn cheque_tenders_carry_their_number_bank_and_date() {
        let mut doc = ticket(Lang::Fr, TaxRegime::Reel);
        doc.stamp_duty = Money::ZERO;
        doc.total_due = Money(120_000);
        doc.paid_total = Money(120_000);
        doc.change_given = Money::ZERO;
        doc.payments = vec![Payment {
            reference: Some("0012345".into()),
            cheque_bank: Some("BNA".into()),
            cheque_date: Some("2026-09-28".into()),
            ..payment(PaymentMethod::Cheque, 120_000)
        }];
        let company = company();
        let r = build(&source(&doc, &company, false)).unwrap();
        assert_eq!(
            r.payments,
            [ReceiptPayment {
                method: PaymentMethod::Cheque,
                amount: Money(120_000),
                reference: Some("0012345".into()),
                cheque_bank: Some("BNA".into()),
                cheque_date: Some("2026-09-28".into()),
            }]
        );
        let json = serde_json::to_value(&r.payments[0]).unwrap();
        assert_eq!(
            (json["chequeBank"].as_str(), json["chequeDate"].as_str()),
            (Some("BNA"), Some("2026-09-28"))
        );
        let fr = labels(Lang::Fr, DocType::Ticket);
        assert_eq!(
            (fr.cheque_bank.as_str(), fr.cheque_date.as_str()),
            ("Banque", "Date du chèque")
        );
        let ar = labels(Lang::Ar, DocType::Ticket);
        assert_eq!(
            (ar.cheque_bank.as_str(), ar.cheque_date.as_str()),
            ("البنك", "تاريخ الصك")
        );
        let en = labels(Lang::En, DocType::Ticket);
        assert_eq!(
            (en.cheque_bank.as_str(), en.cheque_date.as_str()),
            ("Bank", "Cheque date")
        );
    }

    #[test]
    fn arabic_receipts_are_rtl_with_arabic_names_and_words() {
        let (doc, company) = (ticket(Lang::Ar, TaxRegime::Reel), company());
        let r = build(&source(&doc, &company, false)).unwrap();
        assert!(r.rtl);
        assert_eq!(r.header.name, "مغازة الأمل");
        assert_eq!(
            r.header.header_text.as_deref(),
            Some("Ouvert 7j/7"),
            "falls back to French"
        );
        assert_eq!(r.footer_text.as_deref(), Some("شكرا"));
        assert_eq!(r.total_in_letters, "ألف ومائتان واثنا عشر ديناراً");
        let l = labels(Lang::Ar, DocType::Ticket);
        assert_eq!(
            (l.title.as_str(), l.stamp_duty.as_str()),
            ("تذكرة الصندوق", "حقوق الطابع")
        );
        assert_eq!(method_label(Lang::Ar, PaymentMethod::Cash), "نقدا");
    }

    #[test]
    fn ifu_receipts_show_ttc_only_with_the_mention() {
        let (doc, company) = (ticket(Lang::Fr, TaxRegime::Ifu), company());
        let r = build(&source(&doc, &company, false)).unwrap();
        assert!(!r.show_tax && r.ifu_mention && r.taxes.is_empty());
        assert_eq!(
            labels(Lang::Fr, DocType::Ticket).ifu_mention,
            "Régime IFU – TVA non applicable"
        );
    }

    pub(crate) fn sample_receipt(lang: Lang) -> Receipt {
        let (doc, company) = (ticket(lang, TaxRegime::Reel), company());
        build(&source(&doc, &company, false)).unwrap()
    }

    #[test]
    fn a_ticket_can_carry_its_customer_and_balance() {
        let base = sample_receipt(Lang::Fr);
        assert_eq!(base.customer, None);
        let r = base.with_customer(
            ReceiptCustomer {
                name: "Karim Bensalah".into(),
                code: "C00001".into(),
                phone: Some("0555123456".into()),
                address: Some("Bab Ezzouar".into()),
            },
            Some(Money(300_000)),
        );
        assert_eq!(r.customer.as_ref().map(|c| c.code.as_str()), Some("C00001"));
        assert_eq!(r.customer_balance, Some(Money(300_000)));
        assert_eq!(
            label(Lang::Fr, "receipt.customerBalance"),
            "Solde de votre compte"
        );
    }

    #[test]
    fn drafts_have_no_receipt() {
        let mut doc = ticket(Lang::Fr, TaxRegime::Reel);
        doc.status = DocStatus::Draft;
        let company = company();
        assert_eq!(
            build(&source(&doc, &company, false)).unwrap_err().code(),
            "document_not_validated"
        );
    }

    #[test]
    fn every_label_exists_in_every_language() {
        for lang in Lang::ALL {
            for doc_type in [DocType::Ticket, DocType::ReturnNote] {
                let json = serde_json::to_value(labels(lang, doc_type)).unwrap();
                for (field, value) in json.as_object().unwrap() {
                    let text = value.as_str().unwrap();
                    let fallback = text.starts_with("receipt.") || text.starts_with("doc.");
                    assert!(!text.is_empty() && !fallback, "{lang:?} {field}: {text}");
                }
            }
            for method in PaymentMethod::ALL {
                assert!(
                    !method_label(lang, method).starts_with("receipt."),
                    "{lang:?} {method:?}"
                );
            }
            for key in [
                "z.title",
                "z.expected",
                "z.counted",
                "z.variance",
                "z.grandTotal",
                "z.journal",
            ] {
                assert!(
                    locales::text(lang.as_str(), key).is_some(),
                    "{lang:?} {key}"
                );
            }
        }
    }
}
