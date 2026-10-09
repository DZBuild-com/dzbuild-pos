//! The print model of a commercial document (invoice, proforma, quote, delivery note, credit
//! note, purchase order, goods receipt, supplier invoice, transfer note): identities with their
//! mandatory mentions, lines, per-rate totals, duty, net to pay, the amount in letters, payment
//! mode, cited documents, legal lines, marks — in the print language, and paired in the second
//! one for a bilingual document (UX-042).

use super::labels::{DocumentLabels, document_labels, fill, title, word};
use super::qr::{QrMatrix, qr};
use crate::letters::amount_in_letters;
use crate::receipt::method_label;
use pos_core::billing::print::PrintContext;
use pos_core::billing::settings::DocumentLayout;
use pos_core::billing::state::DocState;
use pos_core::company::{PriceMode, TaxRegime};
use pos_core::datapack::Pack;
use pos_core::documents::{CreditKind, DocStatus, DocType, DocumentParty};
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::money::{BP_SCALE, Money, Qty, Rate};
use pos_core::payments::PaymentMethod;
use pos_core::tax::timbre::StampMode;
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum IdKind {
    Rc,
    AeCard,
    Nif,
    Nis,
    Ai,
    Nin,
    Rib,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PartyId {
    pub kind: IdKind,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PartyBlock {
    pub name: String,
    pub name_alt: Option<String>,
    pub legal_form: Option<String>,
    pub activity: Option<String>,
    pub activity_alt: Option<String>,
    pub capital: Option<Money>,
    pub address: Option<String>,
    /// The wilaya's name in the print language.
    pub wilaya: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    /// Printed in this order: RC (or the auto-entrepreneur card), NIF, NIS, AI, NIN, RIB.
    pub ids: Vec<PartyId>,
    pub vat_exempt_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrintComponent {
    pub description: String,
    pub qty: Qty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrintLine {
    pub no: u32,
    pub description: String,
    pub description_alt: Option<String>,
    pub qty: Qty,
    pub unit: String,
    pub unit_price_ht: Money,
    pub unit_price_ttc: Money,
    pub discount: Money,
    pub discount_bp: Rate,
    pub rate_bp: Rate,
    pub total_ht: Money,
    pub total_ttc: Money,
    pub components: Vec<PrintComponent>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrintTax {
    pub rate_bp: Rate,
    /// "TVA 19 %".
    pub label: String,
    pub base_ht: Money,
    pub tax: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrintTotals {
    pub subtotal_ht: Money,
    pub discount_total: Money,
    pub taxes: Vec<PrintTax>,
    pub tax_total: Money,
    pub total_ttc: Money,
    pub stamp_duty: Money,
    pub stamp_absorbed: Money,
    pub rounding: Money,
    /// "Net à payer".
    pub total_due: Money,
    pub paid: Money,
    /// Left on the account (the credit part).
    pub remaining: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrintPayment {
    pub method: PaymentMethod,
    pub label: String,
    pub amount: Money,
    pub reference: Option<String>,
    pub cheque_bank: Option<String>,
    pub cheque_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrintReference {
    pub label: String,
    pub number: String,
    /// `dd/mm/yyyy`.
    pub date: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum WatermarkKind {
    Cancelled,
    Draft,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Watermark {
    pub kind: WatermarkKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SecondLanguage {
    pub lang: Lang,
    pub rtl: bool,
    pub title: String,
    pub labels: DocumentLabels,
    pub in_letters: Option<String>,
    pub legal: Vec<String>,
    pub tax_labels: Vec<String>,
    pub payment_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrintTransfer {
    pub from: String,
    pub to: String,
    pub driver: Option<String>,
    pub vehicle: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CounterpartRole {
    Customer,
    Supplier,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPrintModel {
    pub doc_type: DocType,
    pub lang: Lang,
    pub rtl: bool,
    pub title: String,
    pub number: Option<String>,
    pub date: String,
    pub valid_until: Option<String>,
    pub due_date: Option<String>,
    pub seller: PartyBlock,
    pub counterpart: Option<PartyBlock>,
    pub counterpart_role: CounterpartRole,
    pub show_prices: bool,
    /// Prices are stated HT (décret 05-468 art. 3: "prix unitaire hors taxes; prix total hors
    /// taxes" on every document of a régime-réel shop), an exempt buyer's included.
    pub price_basis_ht: bool,
    /// The TVA column and the per-rate lines (régime réel, not to an exempt buyer).
    pub show_tax: bool,
    pub ifu_mention: Option<String>,
    pub exempt_mention: Option<String>,
    pub lines: Vec<PrintLine>,
    pub totals: PrintTotals,
    pub in_letters: Option<String>,
    pub payments: Vec<PrintPayment>,
    pub payment_mode: Option<String>,
    pub references: Vec<PrintReference>,
    pub credit_kind: Option<String>,
    pub reason: Option<String>,
    pub note: Option<String>,
    pub header_text: Option<String>,
    pub footer_text: Option<String>,
    pub terms_text: Option<String>,
    pub social: Vec<String>,
    /// Non-editable lines of law (proforma, quote validity, delivery note, IFU…).
    pub legal: Vec<String>,
    pub stamp_box: bool,
    /// "DUPLICATA" on every copy after the first (DOC-018).
    pub duplicate: Option<String>,
    /// Diagonal mark: cancelled by credit notes (DOC-007), draft, expired offer.
    pub watermark: Option<Watermark>,
    pub qr: Option<QrMatrix>,
    /// `data:` URL of the shop logo, set by the command.
    pub logo: Option<String>,
    pub layout: DocumentLayout,
    pub labels: DocumentLabels,
    pub second: Option<SecondLanguage>,
    pub transfer: Option<PrintTransfer>,
    pub printed_at: String,
}

/// `YYYY-MM-DD` → `dd/mm/yyyy`; anything else as it is.
pub(crate) fn day(iso: &str) -> String {
    chrono::NaiveDate::parse_from_str(iso, "%Y-%m-%d")
        .map(|d| d.format("%d/%m/%Y").to_string())
        .unwrap_or_else(|_| iso.to_owned())
}

/// Basis points as a percentage: `1900` → "19 %", `1250` → "12,50 %".
fn percent(rate: Rate) -> String {
    let whole = rate.0 / 100;
    match rate.0 % 100 {
        0 => format!("{whole} %"),
        cents => format!("{whole},{cents:02} %"),
    }
}

fn wilaya_name(pack: &Pack, lang: Lang, code: Option<&str>) -> Option<String> {
    let w = pack.wilaya(code?)?;
    Some(match lang {
        Lang::Ar => w.name_ar.clone(),
        Lang::Fr => w.name_fr.clone(),
        Lang::En => w.name_en.clone(),
    })
}

/// The printed block of a party (seller or counterpart) in `lang`: its mentions and identifiers
/// in print order. Public for the pack pages the command layer builds (the instalment contract,
/// the route sheet, the zakat statement).
pub fn party_block(p: &DocumentParty, pack: &Pack, lang: Lang) -> PartyBlock {
    let mut ids = Vec::new();
    let mut push = |kind: IdKind, v: &Option<String>| {
        let value = v.as_deref().map(str::trim).filter(|v| !v.is_empty());
        if let Some(value) = value {
            ids.push(PartyId {
                kind,
                value: value.to_owned(),
            });
        }
        value.is_some()
    };
    if !push(IdKind::Rc, &p.rc) {
        push(IdKind::AeCard, &p.ae_card);
    }
    push(IdKind::Nif, &p.nif);
    push(IdKind::Nis, &p.nis);
    push(IdKind::Ai, &p.ai);
    push(IdKind::Nin, &p.nin);
    push(IdKind::Rib, &p.rib);
    PartyBlock {
        name: p.name.clone(),
        name_alt: p.name_alt.clone(),
        legal_form: p.legal_form.clone(),
        activity: p.activity.clone(),
        activity_alt: p.activity_alt.clone(),
        capital: p.capital,
        address: p.address.clone(),
        wilaya: wilaya_name(pack, lang, p.wilaya.as_deref()),
        phone: p.phone.clone(),
        email: p.email.clone(),
        ids,
        vat_exempt_ref: p.vat_exempt_ref.clone(),
    }
}

/// Legal lines, the amount in letters and the payment mode in one language.
struct Words {
    title: String,
    in_letters: Option<String>,
    legal: Vec<String>,
    payment_mode: Option<String>,
    tax_labels: Vec<String>,
}

/// How an invoice or a credit note was settled (décret 05-468 art. 3: "mode de paiement et date
/// de règlement"): its tenders, then "À terme (règlement le …)" for the part left on the account
/// ("À terme" alone without a due date); the invoice of a ticket names the ticket and the
/// ticket's tenders (the ticket keeps the payments); a credit note lists its refund methods.
fn payment_mode(ctx: &PrintContext, lang: Lang) -> Option<String> {
    let doc = &ctx.document;
    let mut modes: Vec<String> = Vec::new();
    let mut add = |label: String| {
        if !modes.contains(&label) {
            modes.push(label);
        }
    };
    if doc.doc_type == DocType::CreditNote {
        for p in doc
            .payments
            .iter()
            .filter(|p| p.method != PaymentMethod::Change)
        {
            add(method_label(lang, p.method));
        }
        return (!modes.is_empty()).then(|| modes.join(", "));
    }
    let methods: Vec<PaymentMethod> = match &ctx.paid_by {
        Some(p) => p.methods.clone(),
        None => doc.payments.iter().map(|p| p.method).collect(),
    };
    for m in methods
        .iter()
        .filter(|m| !matches!(m, PaymentMethod::Change | PaymentMethod::Credit))
    {
        add(method_label(lang, *m));
    }
    if doc.credit_amount.is_positive() || methods.contains(&PaymentMethod::Credit) {
        add(match doc.due_date.as_deref() {
            Some(due) => fill(lang, "a4.onAccount", &[("date", &day(due))]),
            None => word(lang, "a4.onAccountUndated"),
        });
    }
    if modes.is_empty() {
        return None;
    }
    let modes = modes.join(", ");
    Some(match &ctx.paid_by {
        Some(p) => fill(
            lang,
            "a4.paidByTicket",
            &[
                ("number", &p.reference.number),
                ("date", &day(&p.reference.business_date)),
                ("methods", &modes),
            ],
        ),
        None => modes,
    })
}

fn words(ctx: &PrintContext, lang: Lang, show_tax: bool) -> Words {
    let doc = &ctx.document;
    let lead = match doc.doc_type {
        DocType::Invoice => Some("a4.legal.inLettersInvoice"),
        DocType::CreditNote => Some("a4.legal.inLettersCredit"),
        DocType::Quote | DocType::Proforma => Some("a4.legal.inLettersOther"),
        _ => None,
    };
    let in_letters = lead.map(|key| {
        let amount = doc.total_due.checked_abs().unwrap_or(doc.total_due);
        fill(lang, key, &[("amount", &amount_in_letters(lang, amount))])
    });
    let mut legal = Vec::new();
    match doc.doc_type {
        DocType::Proforma => legal.push(word(lang, "a4.legal.proforma")),
        DocType::DeliveryNote => legal.push(word(lang, "a4.legal.deliveryNotInvoice")),
        _ => {}
    }
    if let (DocType::Quote | DocType::Proforma, Some(v)) = (doc.doc_type, &doc.valid_until) {
        legal.push(fill(lang, "a4.legal.quoteValidity", &[("date", &day(v))]));
    }
    let payment_mode = matches!(doc.doc_type, DocType::Invoice | DocType::CreditNote)
        .then(|| payment_mode(ctx, lang))
        .flatten();
    let tax_labels = if show_tax {
        doc.taxes
            .iter()
            .map(|t| fill(lang, "a4.vat", &[("rate", &percent(t.rate_bp))]))
            .collect()
    } else {
        Vec::new()
    };
    Words {
        title: title(lang, doc.doc_type),
        in_letters,
        legal,
        payment_mode,
        tax_labels,
    }
}

/// The print model of `ctx.document` in `ctx.lang` (paired with `ctx.second_lang`); `duplicate`
/// is the copy number of a reprint (`None` for the original).
pub fn build_document(
    ctx: &PrintContext,
    pack: &Pack,
    duplicate: Option<i64>,
) -> Result<DocumentPrintModel, AppError> {
    let doc = &ctx.document;
    let lang = ctx.lang;
    let reel = doc.tax_regime == TaxRegime::Reel;
    let exempt_ref = ctx
        .counterpart
        .as_ref()
        .and_then(|c| c.vat_exempt_ref.clone())
        .filter(|_| reel && doc.taxes.iter().all(|t| t.rate_bp == Rate::ZERO));
    let price_basis_ht = reel && doc.doc_type != DocType::Transfer;
    let show_tax = price_basis_ht && exempt_ref.is_none();
    let show_prices = match doc.doc_type {
        DocType::DeliveryNote => ctx.settings.layout.show_delivery_prices,
        DocType::Transfer => false,
        _ => true,
    };
    let unit_ht = |unit_price: Money, rate: Rate| -> Money {
        match doc.price_mode {
            PriceMode::Ttc => unit_price
                .mul_div(BP_SCALE, BP_SCALE + i64::from(rate.0))
                .unwrap_or(unit_price),
            PriceMode::Ht => unit_price,
        }
    };
    let unit_ttc = |unit_price: Money, rate: Rate| -> Money {
        match doc.price_mode {
            PriceMode::Ttc => unit_price,
            PriceMode::Ht => unit_price
                .mul_div(BP_SCALE + i64::from(rate.0), BP_SCALE)
                .unwrap_or(unit_price),
        }
    };
    let mut lines: Vec<PrintLine> = Vec::new();
    let mut ids: Vec<&str> = Vec::new();
    for l in &doc.lines {
        if let Some(parent) = &l.parent_line_id {
            if let Some(i) = ids.iter().position(|id| id == parent) {
                lines[i].components.push(PrintComponent {
                    description: l.description.clone(),
                    qty: l.qty,
                });
            }
            continue;
        }
        ids.push(&l.id);
        let description = if l.reason_code.as_deref() == Some("credit_financial") {
            fill(
                lang,
                "a4.legal.financialCredit",
                &[("number", &l.description)],
            )
        } else {
            l.description.clone()
        };
        lines.push(PrintLine {
            no: u32::try_from(lines.len() + 1).unwrap_or(u32::MAX),
            description,
            description_alt: ctx.alt_names.get(&l.id).cloned(),
            qty: l.qty,
            unit: ctx
                .unit_names
                .get(&l.unit_code)
                .cloned()
                .unwrap_or_else(|| l.unit_code.clone()),
            unit_price_ht: unit_ht(l.unit_price, l.tax_rate_bp),
            unit_price_ttc: unit_ttc(l.unit_price, l.tax_rate_bp),
            discount: l.discount_amount,
            discount_bp: l.discount_bp,
            rate_bp: l.tax_rate_bp,
            total_ht: l.total_ht,
            total_ttc: l.total_ttc,
            components: Vec::new(),
            note: l.note.clone(),
        });
    }
    let main = words(ctx, lang, show_tax);
    let invoice = doc.doc_type == DocType::Invoice;
    let paid = doc
        .paid_total
        .checked_sub(doc.change_given)
        .and_then(|p| p.checked_sub(doc.credit_amount))
        .unwrap_or(Money::ZERO);
    let totals = PrintTotals {
        subtotal_ht: doc.subtotal_ht,
        discount_total: doc.discount_total,
        taxes: doc
            .taxes
            .iter()
            .zip(&main.tax_labels)
            .map(|(t, label)| PrintTax {
                rate_bp: t.rate_bp,
                label: label.clone(),
                base_ht: t.base_ht,
                tax: t.tax_amount,
            })
            .collect(),
        tax_total: doc.tax_total,
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
        paid: if invoice { paid } else { Money::ZERO },
        remaining: if invoice {
            doc.credit_amount
        } else {
            Money::ZERO
        },
    };
    let watermark = match (&ctx.state, doc.status) {
        (_, DocStatus::Draft) => Some(Watermark {
            kind: WatermarkKind::Draft,
            text: word(lang, "a4.watermark.draft"),
        }),
        (DocState::Credited { by }, _) => Some(Watermark {
            kind: WatermarkKind::Cancelled,
            text: fill(
                lang,
                "a4.watermark.cancelled",
                &[("numbers", &by.join(", "))],
            ),
        }),
        (DocState::Expired { .. }, _) => Some(Watermark {
            kind: WatermarkKind::Expired,
            text: word(lang, "a4.watermark.expired"),
        }),
        _ => None,
    };
    let stamp_box = matches!(doc.doc_type, DocType::Invoice | DocType::CreditNote)
        || ctx.settings.layout.show_stamp_box;
    let second = ctx.second_lang.map(|l| {
        let w = words(ctx, l, show_tax);
        SecondLanguage {
            lang: l,
            rtl: l.is_rtl(),
            title: w.title,
            labels: document_labels(l),
            in_letters: w.in_letters,
            legal: w.legal,
            tax_labels: w.tax_labels,
            payment_mode: w.payment_mode,
        }
    });
    let reason = doc
        .lines
        .iter()
        .find_map(|l| {
            l.reason_code
                .as_deref()
                .filter(|r| *r != "credit_financial")
        })
        .filter(|_| doc.doc_type == DocType::CreditNote)
        .map(|r| word(lang, &format!("a4.creditReasons.{r}")));
    let credit_kind = doc.credit_kind.map(|k| {
        word(
            lang,
            match k {
                CreditKind::Goods => "a4.creditKinds.goods",
                CreditKind::Financial => "a4.creditKinds.financial",
            },
        )
    });
    Ok(DocumentPrintModel {
        doc_type: doc.doc_type,
        lang,
        rtl: lang.is_rtl(),
        title: main.title,
        number: doc.number.clone(),
        date: day(&doc.business_date),
        valid_until: doc.valid_until.as_deref().map(day),
        due_date: doc.due_date.as_deref().map(day),
        seller: party_block(&ctx.seller, pack, lang),
        counterpart: ctx.counterpart.as_ref().map(|c| party_block(c, pack, lang)),
        counterpart_role: if ctx.counterpart_is_supplier {
            CounterpartRole::Supplier
        } else {
            CounterpartRole::Customer
        },
        show_prices,
        price_basis_ht,
        show_tax,
        ifu_mention: (!reel).then(|| word(lang, "receipt.ifuMention")),
        exempt_mention: exempt_ref.map(|r| fill(lang, "a4.legal.exempt", &[("reference", &r)])),
        lines,
        totals,
        in_letters: main.in_letters,
        payments: doc
            .payments
            .iter()
            .filter(|p| p.method != PaymentMethod::Change)
            .map(|p| PrintPayment {
                method: p.method,
                label: method_label(lang, p.method),
                amount: p.amount,
                reference: p.reference.clone(),
                cheque_bank: p.cheque_bank.clone(),
                cheque_date: p.cheque_date.clone(),
            })
            .collect(),
        payment_mode: main.payment_mode,
        references: ctx
            .references
            .iter()
            .map(|r| PrintReference {
                label: title(lang, r.doc_type),
                number: r.number.clone(),
                date: day(&r.business_date),
            })
            .collect(),
        credit_kind,
        reason,
        note: doc.note.clone(),
        header_text: ctx.settings.texts.header.in_lang(lang).map(str::to_owned),
        footer_text: ctx.settings.texts.footer.in_lang(lang).map(str::to_owned),
        terms_text: ctx.settings.texts.terms.in_lang(lang).map(str::to_owned),
        social: ctx.settings.social.clone(),
        legal: main.legal,
        stamp_box,
        duplicate: duplicate.map(|_| word(lang, "a4.watermark.duplicate")),
        watermark,
        qr: doc
            .number
            .as_deref()
            .filter(|_| doc.status == DocStatus::Validated)
            .and_then(qr),
        logo: None,
        layout: ctx.settings.layout.clone(),
        labels: document_labels(lang),
        second,
        transfer: ctx.transfer.as_ref().map(|t| PrintTransfer {
            from: t.from.clone(),
            to: t.to.clone(),
            driver: t.driver_name.clone(),
            vehicle: t.vehicle.clone(),
        }),
        printed_at: pos_core::time::algiers_datetime(ctx.printed_at)
            .format("%d/%m/%Y %H:%M")
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a4::testdata;

    fn pack() -> Pack {
        pos_core::datapack::bundled().unwrap()
    }

    fn model(ctx: &PrintContext) -> DocumentPrintModel {
        build_document(ctx, &pack(), None).unwrap()
    }

    #[test]
    fn every_invoice_model_carries_the_mandatory_mentions() {
        let m = model(&testdata::invoice());
        assert_eq!(
            (m.title.as_str(), m.number.as_deref(), m.date.as_str()),
            ("Facture", Some("FA2026-000045"), "28/09/2026")
        );
        let ids = |p: &PartyBlock| {
            p.ids
                .iter()
                .map(|i| (i.kind, i.value.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(&m.seller),
            [
                (IdKind::Rc, "16/00-1234567B26".into()),
                (IdKind::Nif, "000116001234567".into()),
                (IdKind::Nis, "000116001234567".into()),
                (IdKind::Ai, "16012345678".into()),
                (IdKind::Rib, "00799999001234567890".into()),
            ]
        );
        assert_eq!(m.seller.legal_form.as_deref(), Some("SARL"));
        assert_eq!(m.seller.capital, Some(Money(100_000_000)));
        assert_eq!(
            m.seller.activity.as_deref(),
            Some("Commerce de détail en alimentation générale")
        );
        assert_eq!(m.seller.wilaya.as_deref(), Some("Alger"));
        let buyer = m.counterpart.as_ref().unwrap();
        assert_eq!(ids(buyer).len(), 4, "RC, NIF, NIS and AI of the buyer");
        assert!(m.show_tax);
        assert_eq!(
            m.lines[0].unit_price_ht,
            Money(100_000),
            "P.U. HT of a TTC price"
        );
        assert_eq!(m.lines[0].unit, "pièce");
        assert_eq!(
            m.totals
                .taxes
                .iter()
                .map(|t| (t.label.as_str(), t.tax))
                .collect::<Vec<_>>(),
            [("TVA 19 %", Money(199_500)), ("TVA 9 %", Money(90_000))]
        );
        assert_eq!(
            (m.totals.stamp_duty, m.totals.total_due),
            (Money(5_400), Money(2_344_900))
        );
        assert_eq!(
            (m.totals.paid, m.totals.remaining),
            (Money(544_900), Money(1_800_000))
        );
        assert_eq!(
            m.in_letters.as_deref(),
            Some(
                "Arrêtée la présente facture à la somme de : vingt-trois mille quatre cent quarante-neuf dinars"
            )
        );
        assert_eq!(
            m.payment_mode.as_deref(),
            Some("Espèces, À terme (règlement le 28/10/2026)")
        );
        assert!(m.stamp_box);
        assert_eq!(m.qr.as_ref().map(|q| q.size), Some(21));
        assert_eq!(m.logo, None);
        insta::assert_json_snapshot!("invoice_fr", m);
    }

    #[test]
    fn an_ifu_invoice_never_shows_tva() {
        let m = model(&testdata::ifu());
        assert!(!m.show_tax);
        assert!(m.totals.taxes.is_empty());
        assert_eq!(
            m.ifu_mention.as_deref(),
            Some("Régime IFU – TVA non applicable")
        );
        assert_eq!(m.lines[0].unit_price_ttc, Money(119_000));
        let json = serde_json::to_string(&m).unwrap();
        assert!(!json.contains("TVA 19"), "no TVA label anywhere");
    }

    #[test]
    fn an_exempt_buyer_gets_the_exemption_and_no_tva() {
        let m = model(&testdata::exempt());
        assert!(!m.show_tax, "no TVA column, no TVA line");
        assert!(
            m.price_basis_ht,
            "a réel shop states its prices HT, to an exempt buyer too (décret 05-468 art. 3)"
        );
        let ifu = model(&testdata::ifu());
        assert!(!ifu.price_basis_ht, "an IFU shop prices TTC");
        assert_eq!(
            m.exempt_mention.as_deref(),
            Some("Exonéré de TVA — attestation n° ATT-2026-114")
        );
    }

    #[test]
    fn a_bilingual_invoice_pairs_both_languages() {
        let m = model(&testdata::bilingual());
        let second = m.second.as_ref().unwrap();
        assert_eq!(
            (second.lang, second.rtl, second.title.as_str()),
            (Lang::Ar, true, "فاتورة")
        );
        assert_eq!(second.labels.nif, "رقم التعريف الجبائي");
        assert!(
            second
                .in_letters
                .as_deref()
                .unwrap()
                .starts_with("أوقفت هذه الفاتورة على مبلغ: ")
        );
        assert_eq!(m.lines[0].description_alt.as_deref(), Some("إسمنت 25 كغ"));
        assert_eq!(m.seller.name_alt.as_deref(), Some("مغازة الأمل"));
        insta::assert_json_snapshot!("invoice_bilingual", m);
    }

    #[test]
    fn arabic_and_english_invoices_are_whole() {
        let ar = model(&testdata::invoice_in(Lang::Ar));
        assert!(ar.rtl);
        assert_eq!(ar.title, "فاتورة");
        assert_eq!(ar.totals.taxes[0].label, "الرسم على القيمة المضافة 19 %");
        assert!(ar.in_letters.as_deref().unwrap().contains("دينار"));
        let en = model(&testdata::invoice_in(Lang::En));
        assert_eq!(en.labels.net_to_pay, "Amount due");
        assert_eq!(
            en.payment_mode.as_deref(),
            Some("Cash, On account (due 28/10/2026)")
        );
    }

    #[test]
    fn a_proforma_says_it_is_not_an_invoice_and_a_quote_how_long_it_holds() {
        let pf = model(&testdata::quote(DocType::Proforma));
        assert_eq!(pf.title, "Facture proforma");
        assert!(
            pf.legal
                .contains(&"FACTURE PROFORMA — ne constitue pas une facture".to_owned())
        );
        assert!(pf.payment_mode.is_none());
        let dv = model(&testdata::quote(DocType::Quote));
        assert!(
            dv.legal
                .contains(&"Offre valable jusqu'au 28/10/2026.".to_owned())
        );
        let bl = model(&testdata::delivery());
        assert!(
            bl.legal
                .contains(&"Ce bon de livraison n'est pas une facture.".to_owned())
        );
        assert!(bl.in_letters.is_none());
    }

    #[test]
    fn a_fully_credited_invoice_prints_cancelled_and_a_reprint_duplicata() {
        let m = model(&testdata::cancelled());
        assert_eq!(
            m.watermark,
            Some(Watermark {
                kind: WatermarkKind::Cancelled,
                text: "FACTURE ANNULÉE — avoir AV2026-000003".into()
            })
        );
        let dup = build_document(&testdata::invoice(), &pack(), Some(2)).unwrap();
        assert_eq!(dup.duplicate.as_deref(), Some("DUPLICATA"));
        let mut draft = testdata::invoice();
        draft.document.status = pos_core::documents::DocStatus::Draft;
        draft.document.number = None;
        let m = model(&draft);
        assert_eq!(m.watermark.map(|w| w.kind), Some(WatermarkKind::Draft));
        assert!(m.qr.is_none());
    }

    #[test]
    fn a_credit_note_cites_its_invoice_and_its_kind() {
        let m = model(&testdata::credit_note());
        assert_eq!(m.title, "Avoir");
        assert_eq!(m.credit_kind.as_deref(), Some("Avoir marchandise"));
        assert_eq!(m.reason.as_deref(), Some("Article défectueux"));
        assert_eq!(
            m.references,
            [PrintReference {
                label: "Facture".into(),
                number: "FA2026-000045".into(),
                date: "28/09/2026".into()
            }]
        );
        assert!(
            m.in_letters
                .as_deref()
                .unwrap()
                .starts_with("Arrêté le présent avoir à la somme de : trois mille")
        );
    }

    #[test]
    fn a_reprint_after_the_company_moved_shows_the_old_address() {
        // The context carries the snapshot (Task 9 pins that it does); the model prints it as is.
        let mut ctx = testdata::invoice();
        ctx.seller.address = Some("12 rue Larbi Ben M'hidi, Alger".into());
        let m = model(&ctx);
        assert_eq!(
            m.seller.address.as_deref(),
            Some("12 rue Larbi Ben M'hidi, Alger")
        );
    }

    #[test]
    fn an_invoice_of_a_kridi_ticket_names_the_ticket_its_tenders_and_the_part_on_account() {
        let mut ctx = testdata::from_ticket();
        ctx.second_lang = Some(Lang::Ar);
        let m = model(&ctx);
        assert_eq!(
            m.payment_mode.as_deref(),
            Some("Ticket de caisse TK2026-C1-000123 du 27/09/2026 : Espèces, À terme")
        );
        assert_eq!(
            m.second.as_ref().and_then(|s| s.payment_mode.as_deref()),
            Some("تذكرة الصندوق TK2026-C1-000123 بتاريخ 27/09/2026: نقدا, بالآجل")
        );
        assert_eq!(
            (m.totals.paid, m.totals.remaining),
            (Money(544_900), Money(1_800_000))
        );
        ctx.lang = Lang::En;
        ctx.second_lang = None;
        assert_eq!(
            model(&ctx).payment_mode.as_deref(),
            Some("Receipt TK2026-C1-000123 of 27/09/2026: Cash, On account")
        );
    }

    #[test]
    fn an_invoice_of_a_ticket_lists_each_tender_once_and_the_account_part_from_either_side() {
        let mode = |methods: Vec<PaymentMethod>, credit: i64| {
            let mut ctx = testdata::from_ticket();
            ctx.document.credit_amount = Money(credit);
            if let Some(p) = ctx.paid_by.as_mut() {
                p.methods = methods;
            }
            model(&ctx).payment_mode
        };
        let ticket = "Ticket de caisse TK2026-C1-000123 du 27/09/2026 : ";
        assert_eq!(
            mode(
                vec![
                    PaymentMethod::Cash,
                    PaymentMethod::Change,
                    PaymentMethod::CardCib,
                    PaymentMethod::Cash
                ],
                0
            ),
            Some(format!("{ticket}Espèces, Carte CIB")),
            "a cash ticket states its tenders, never the change"
        );
        assert_eq!(
            mode(vec![PaymentMethod::Cash], 1_800_000),
            Some(format!("{ticket}Espèces, À terme")),
            "the part on account shows even when the methods miss it"
        );
        assert_eq!(
            mode(vec![PaymentMethod::Credit], 0),
            Some(format!("{ticket}À terme"))
        );
        assert_eq!(mode(Vec::new(), 0), None, "nothing was tendered");
    }

    #[test]
    fn an_invoice_on_account_without_a_due_date_prints_no_empty_date() {
        let mut ctx = testdata::invoice();
        ctx.document.due_date = None;
        assert_eq!(
            model(&ctx).payment_mode.as_deref(),
            Some("Espèces, À terme")
        );
        ctx.lang = Lang::Ar;
        assert_eq!(model(&ctx).payment_mode.as_deref(), Some("نقدا, بالآجل"));
        ctx.lang = Lang::En;
        assert_eq!(
            model(&ctx).payment_mode.as_deref(),
            Some("Cash, On account")
        );
    }

    #[test]
    fn an_auto_entrepreneur_prints_his_card_instead_of_a_blank_rc() {
        let mut ctx = testdata::invoice();
        ctx.seller.rc = Some("  ".into());
        ctx.seller.ae_card = Some("AE-16-000123".into());
        let m = model(&ctx);
        assert_eq!(
            m.seller.ids.first(),
            Some(&PartyId {
                kind: IdKind::AeCard,
                value: "AE-16-000123".into()
            })
        );
        assert!(!m.seller.ids.iter().any(|i| i.kind == IdKind::Rc));
    }
}
