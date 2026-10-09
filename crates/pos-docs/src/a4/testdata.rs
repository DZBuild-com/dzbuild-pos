//! Hand-built print contexts for the model and template tests: one invoice of "Supérette El
//! Amel" to "SARL Amine" (10 bags of cement at 1 190.00 with 5 % off, 2 m3 of sand at 5 450.00,
//! a transport line: 23 395.00 TTC), paid 5 449.00 cash (duty 54.00 on the 5 395.00 of TTC paid
//! in cash) + 18 000.00 on account, and its variants.

use pos_core::billing::print::{PaidBy, PrintContext, Reference};
use pos_core::billing::settings::{DocumentLayout, DocumentTexts, PrintSettings, Trilingual};
use pos_core::billing::state::DocState;
use pos_core::company::{PriceMode, TaxRegime};
use pos_core::documents::{
    DocStatus, DocType, Document, DocumentLine, DocumentParty, DocumentTax, PartyRole, Payment,
};
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty, Rate};
use pos_core::payments::PaymentMethod;
use pos_core::tax::timbre::StampMode;
use std::collections::BTreeMap;

/// 2026-09-28 10:15 Algiers.
pub const PRINTED_AT: i64 = 1_790_586_900_000;

/// One line as the invoice table reads it: number, id, name, quantity, unit price, discount,
/// rate, HT and TVA.
#[allow(clippy::too_many_arguments)]
fn line(
    no: i64,
    id: &str,
    description: &str,
    qty: i64,
    unit_price: i64,
    discount: i64,
    rate: i32,
    ht: i64,
    tax: i64,
) -> DocumentLine {
    DocumentLine {
        id: id.into(),
        line_no: no,
        product_id: Some(format!("p-{id}")),
        description: description.into(),
        description_alt: None,
        qty: Qty(qty),
        unit_code: "pc".into(),
        unit_price: Money(unit_price),
        price_overridden: false,
        discount_bp: Rate(if discount > 0 { 500 } else { 0 }),
        discount_amount: Money(discount),
        tax_rate_bp: Rate(rate),
        total_ht: Money(ht),
        tax_amount: Money(tax),
        total_ttc: Money(ht + tax),
        cost_price: Money::ZERO,
        lot_id: None,
        serial_no: None,
        parent_line_id: None,
        origin_line_id: None,
        restock_location_id: None,
        reason_code: None,
        note: None,
    }
}

pub fn seller() -> DocumentParty {
    DocumentParty {
        role: PartyRole::Seller,
        name: "Supérette El Amel".into(),
        name_alt: Some("مغازة الأمل".into()),
        legal_form: Some("SARL".into()),
        activity: Some("Commerce de détail en alimentation générale".into()),
        activity_alt: Some("تجارة التجزئة للمواد الغذائية العامة".into()),
        capital: Some(Money(100_000_000)),
        address: Some("12 rue Larbi Ben M'hidi, Alger".into()),
        wilaya: Some("16".into()),
        phone: Some("023 45 67 89".into()),
        rc: Some("16/00-1234567B26".into()),
        nif: Some("000116001234567".into()),
        nis: Some("000116001234567".into()),
        ai: Some("16012345678".into()),
        rib: Some("00799999001234567890".into()),
        ..DocumentParty::default()
    }
}

pub fn buyer() -> DocumentParty {
    DocumentParty {
        role: PartyRole::Buyer,
        party_id: Some("c-amine".into()),
        name: "SARL Amine".into(),
        legal_form: Some("SARL".into()),
        activity: Some("Quincaillerie".into()),
        address: Some("5 rue Didouche Mourad, Alger".into()),
        wilaya: Some("16".into()),
        rc: Some("16/00-7654321B26".into()),
        nif: Some("000016001234567".into()),
        nis: Some("000016007654321".into()),
        ai: Some("16098765432".into()),
        ..DocumentParty::default()
    }
}

pub fn invoice_document() -> Document {
    let lines = vec![
        line(
            1,
            "l1",
            "Ciment 25 kg",
            10_000,
            119_000,
            59_500,
            1_900,
            950_000,
            180_500,
        ),
        line(
            2,
            "l2",
            "Sable 1 m3",
            2_000,
            545_000,
            0,
            900,
            1_000_000,
            90_000,
        ),
        {
            let mut t = line(
                3,
                "l3",
                "Transport Alger – Blida",
                1_000,
                119_000,
                0,
                1_900,
                100_000,
                19_000,
            );
            t.product_id = None;
            t
        },
    ];
    Document {
        id: "inv-1".into(),
        doc_type: DocType::Invoice,
        number: Some("FA2026-000045".into()),
        series: String::new(),
        fiscal_year: Some(2026),
        status: DocStatus::Validated,
        business_date: "2026-09-28".into(),
        customer_id: Some("c-amine".into()),
        supplier_id: None,
        user_id: "u-owner".into(),
        terminal_id: "t-1".into(),
        cash_session_id: Some("s-1".into()),
        price_level_id: None,
        lang: Lang::Fr,
        tax_regime: TaxRegime::Reel,
        price_mode: PriceMode::Ttc,
        subtotal_ht: Money(2_050_000),
        discount_total: Money(59_500),
        tax_total: Money(289_500),
        total_ttc: Money(2_339_500),
        stamp_duty: Money(5_400),
        stamp_mode: StampMode::ChargeCustomer,
        rounding: Money::ZERO,
        total_due: Money(2_344_900),
        paid_total: Money(2_344_900),
        change_given: Money::ZERO,
        credit_amount: Money(1_800_000),
        note: None,
        parent_id: None,
        source_ref: None,
        validated_at: Some(PRINTED_AT),
        chain_seq: Some(45),
        prev_hash: Some("0".repeat(64)),
        hash: Some("a".repeat(64)),
        created_at: PRINTED_AT,
        valid_until: None,
        due_date: Some("2026-10-28".into()),
        second_lang: None,
        credit_kind: None,
        origin: Some(pos_core::documents::DocOrigin::Direct),
        parties: vec![seller(), buyer()],
        lines,
        taxes: vec![
            DocumentTax {
                rate_bp: Rate(1_900),
                base_ht: Money(1_050_000),
                tax_amount: Money(199_500),
            },
            DocumentTax {
                rate_bp: Rate(900),
                base_ht: Money(1_000_000),
                tax_amount: Money(90_000),
            },
        ],
        payments: vec![
            Payment {
                id: "p1".into(),
                method: PaymentMethod::Cash,
                amount: Money(544_900),
                reference: None,
                cheque_bank: None,
                cheque_date: None,
                cash_session_id: Some("s-1".into()),
            },
            Payment {
                id: "p2".into(),
                method: PaymentMethod::Credit,
                amount: Money(1_800_000),
                reference: None,
                cheque_bank: None,
                cheque_date: None,
                cash_session_id: Some("s-1".into()),
            },
        ],
    }
}

pub fn settings() -> PrintSettings {
    PrintSettings {
        layout: DocumentLayout::default(),
        texts: DocumentTexts {
            footer: Trilingual {
                ar: "شكراً لثقتكم".into(),
                fr: "Merci de votre confiance".into(),
                en: "Thank you for your trust".into(),
            },
            terms: Trilingual {
                ar: "البضاعة المباعة لا تُرد إلا بإشعار دائن.".into(),
                fr: "Marchandise reprise uniquement par avoir.".into(),
                en: "Goods are taken back by credit note only.".into(),
            },
            warranty: Trilingual {
                ar: "يغطي الضمان عيوب الصنع، ولا يشمل الكسر والأكسدة.".into(),
                fr: "Garantie contre les défauts de fabrication, hors casse et oxydation.".into(),
                en: "Covers manufacturing defects, not breakage or oxidation.".into(),
            },
            ..DocumentTexts::default()
        },
        activity: Trilingual::default(),
        ae_card: None,
        social: vec!["elamel.dz".into()],
    }
}

pub fn context(document: Document) -> PrintContext {
    let counterpart = document
        .parties
        .iter()
        .find(|p| p.role == PartyRole::Buyer)
        .cloned();
    PrintContext {
        lang: document.lang,
        second_lang: document.second_lang,
        seller: document.parties.first().cloned().unwrap_or_else(seller),
        counterpart,
        counterpart_is_supplier: false,
        settings: settings(),
        references: Vec::new(),
        paid_by: None,
        state: DocState::Unpaid {
            remaining: Money(1_800_000),
            due_date: Some("2026-10-28".into()),
            overdue: false,
        },
        alt_names: BTreeMap::new(),
        unit_names: BTreeMap::from([("pc".to_owned(), "pièce".to_owned())]),
        transfer: None,
        has_logo: false,
        today: "2026-09-28".into(),
        printed_at: PRINTED_AT,
        document,
    }
}

pub fn invoice() -> PrintContext {
    context(invoice_document())
}

pub fn invoice_in(lang: Lang) -> PrintContext {
    let mut c = invoice();
    c.lang = lang;
    if lang == Lang::Ar {
        c.seller.name = "مغازة الأمل".into();
        c.seller.activity = Some("تجارة التجزئة للمواد الغذائية العامة".into());
        c.unit_names = BTreeMap::from([("pc".to_owned(), "قطعة".to_owned())]);
    }
    if lang == Lang::En {
        c.unit_names = BTreeMap::from([("pc".to_owned(), "piece".to_owned())]);
    }
    c
}

pub fn bilingual() -> PrintContext {
    let mut c = invoice();
    c.second_lang = Some(Lang::Ar);
    c.alt_names = BTreeMap::from([
        ("l1".to_owned(), "إسمنت 25 كغ".to_owned()),
        ("l2".to_owned(), "رمل 1 م3".to_owned()),
    ]);
    c
}

pub fn ifu() -> PrintContext {
    let mut d = invoice_document();
    d.tax_regime = TaxRegime::Ifu;
    context(d)
}

pub fn exempt() -> PrintContext {
    let mut d = invoice_document();
    for l in &mut d.lines {
        l.tax_rate_bp = Rate::ZERO;
        l.total_ttc = l.total_ht;
        l.tax_amount = Money::ZERO;
    }
    d.taxes = vec![DocumentTax {
        rate_bp: Rate::ZERO,
        base_ht: Money(2_050_000),
        tax_amount: Money::ZERO,
    }];
    d.tax_total = Money::ZERO;
    d.total_ttc = Money(2_050_000);
    d.parties[1].vat_exempt_ref = Some("ATT-2026-114".into());
    context(d)
}

pub fn cancelled() -> PrintContext {
    let mut c = invoice();
    c.state = DocState::Credited {
        by: vec!["AV2026-000003".into()],
    };
    c
}

/// The invoice of a kridi ticket (SAL-054): the ticket keeps its payments (5 449.00 cash and
/// 18 000.00 on account), the invoice copies its totals and has no due date of its own.
pub fn from_ticket() -> PrintContext {
    let mut d = invoice_document();
    d.number = Some("FA2026-000046".into());
    d.origin = Some(pos_core::documents::DocOrigin::Ticket);
    d.cash_session_id = None;
    d.due_date = None;
    d.payments.clear();
    let ticket = Reference {
        doc_type: DocType::Ticket,
        number: "TK2026-C1-000123".into(),
        business_date: "2026-09-27".into(),
    };
    let mut c = context(d);
    c.references = vec![ticket.clone()];
    c.paid_by = Some(PaidBy {
        reference: ticket,
        methods: vec![PaymentMethod::Cash, PaymentMethod::Credit],
    });
    c.state = DocState::Unpaid {
        remaining: Money(1_800_000),
        due_date: None,
        overdue: false,
    };
    c
}

pub fn quote(doc_type: DocType) -> PrintContext {
    let mut d = invoice_document();
    d.doc_type = doc_type;
    d.number = Some(if doc_type == DocType::Quote {
        "DV2026-000012".into()
    } else {
        "PF2026-000004".into()
    });
    d.valid_until = Some("2026-10-28".into());
    d.due_date = None;
    d.payments.clear();
    d.stamp_duty = Money::ZERO;
    d.total_due = d.total_ttc;
    d.paid_total = Money::ZERO;
    d.credit_amount = Money::ZERO;
    let mut c = context(d);
    c.state = DocState::Open {
        valid_until: Some("2026-10-28".into()),
    };
    c
}

pub fn delivery() -> PrintContext {
    let mut c = quote(DocType::DeliveryNote);
    c.document.number = Some("BL2026-000031".into());
    c.document.valid_until = None;
    c.state = DocState::ToInvoice;
    c
}

pub fn credit_note() -> PrintContext {
    let mut d = invoice_document();
    d.doc_type = DocType::CreditNote;
    d.number = Some("AV2026-000003".into());
    d.credit_kind = Some(pos_core::documents::CreditKind::Goods);
    d.lines.truncate(1);
    for l in &mut d.lines {
        l.qty = Qty(-3_000);
        l.discount_amount = Money(-17_850);
        l.total_ht = Money(-285_000);
        l.tax_amount = Money(-54_150);
        l.total_ttc = Money(-339_150);
        l.reason_code = Some("defective".into());
    }
    d.taxes = vec![DocumentTax {
        rate_bp: Rate(1_900),
        base_ht: Money(-285_000),
        tax_amount: Money(-54_150),
    }];
    d.subtotal_ht = Money(-285_000);
    d.tax_total = Money(-54_150);
    d.discount_total = Money(-17_850);
    d.total_ttc = Money(-339_150);
    d.total_due = Money(-339_150);
    d.stamp_duty = Money::ZERO;
    d.stamp_mode = StampMode::Off;
    d.paid_total = Money(-339_150);
    d.credit_amount = Money::ZERO;
    d.due_date = None;
    d.parent_id = Some("inv-1".into());
    d.payments = vec![Payment {
        id: "p1".into(),
        method: PaymentMethod::Credit,
        amount: Money(-339_150),
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        cash_session_id: None,
    }];
    let mut c = context(d);
    c.references = vec![Reference {
        doc_type: DocType::Invoice,
        number: "FA2026-000045".into(),
        business_date: "2026-09-28".into(),
    }];
    c.state = DocState::Final;
    c
}
