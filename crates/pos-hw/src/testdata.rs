//! Documents the rendering tests print.

use crate::config::{PrintMode, PrinterConfig};
use crate::driver::Transport;
use crate::labels::model::{
    LabelBarcode, LabelData, LabelLang, LabelSymbology, LabelTemplate, WEIGHED_TEMPLATE,
    builtin_templates,
};
use crate::profile::{PaperWidth, Preset, Profile};
use pos_core::cash::closing::{
    ChainAnchor, CountLine, DocAnchor, ExpenseTotal, MethodTotal, SessionSummary, ZReport,
};
use pos_core::cash::{CashSession, SessionStatus};
use pos_core::documents::DocType;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty, Rate};
use pos_core::payments::PaymentMethod;
use pos_core::restaurant::kitchen::{KitchenKind, KitchenLine, KitchenTicket};
use pos_core::restaurant::tickets::DiningMode;
use pos_core::tax::tva::RateTotal;
use pos_docs::receipt::{Receipt, ReceiptHeader, ReceiptLine, ReceiptPayment, ReceiptTax};
use pos_docs::slips::{PreBillLine, PreBillSlip, SlipShop, VoucherSlip, kitchen_slip};

fn by_lang(lang: Lang, fr: &str, ar: &str, en: &str) -> String {
    match lang {
        Lang::Fr => fr,
        Lang::Ar => ar,
        Lang::En => en,
    }
    .to_owned()
}

fn line(
    description: String,
    qty: i64,
    unit: &str,
    price: i64,
    discount: i64,
    total: i64,
    rate: i32,
) -> ReceiptLine {
    ReceiptLine {
        description,
        qty: Qty(qty),
        unit_code: unit.to_owned(),
        unit_price: Money(price),
        discount: Money(discount),
        total: Money(total),
        tax_rate_bp: Rate(rate),
        note: None,
        components: Vec::new(),
    }
}

/// A cash tender (no cheque details).
pub(crate) fn cash(amount: i64) -> ReceiptPayment {
    ReceiptPayment {
        method: PaymentMethod::Cash,
        amount: Money(amount),
        reference: None,
        cheque_bank: None,
        cheque_date: None,
    }
}

/// A régime réel ticket: 2 milks at 9 %, oil at 19 % with a 5 % discount, 1.25 kg of tomatoes,
/// paid 1 200 DA cash, 11 DA duty, 121,50 DA change.
pub(crate) fn ticket(lang: Lang) -> Receipt {
    Receipt {
        lang,
        rtl: lang == Lang::Ar,
        doc_type: DocType::Ticket,
        number: "TK2026-C1-000123".into(),
        local_time: "28/09/2026 12:00".into(),
        cashier: "Yacine".into(),
        terminal_code: "C1".into(),
        header: ReceiptHeader {
            name: by_lang(
                lang,
                "Supérette El Amel",
                "سوبيرات الأمل",
                "El Amel Superette",
            ),
            legal_form: Some("EURL".into()),
            address: Some(by_lang(
                lang,
                "12 rue Larbi Ben M'hidi, Alger",
                "12 شارع العربي بن مهيدي، الجزائر",
                "12 Larbi Ben M'hidi St, Algiers",
            )),
            phone: Some("0550 12 34 56".into()),
            rc: Some("16/00-1234567 B 22".into()),
            nif: Some("001216098765432".into()),
            nis: None,
            ai: None,
            header_text: None,
            demo: false,
        },
        lines: vec![
            line(
                by_lang(
                    lang,
                    "Lait Candia 1 L",
                    "حليب كانديا 1 لتر",
                    "Candia milk 1 L",
                ),
                2_000,
                "pc",
                15_000,
                0,
                30_000,
                900,
            ),
            line(
                by_lang(lang, "Huile Elio 5 L", "زيت إليو 5 لتر", "Elio oil 5 L"),
                1_000,
                "pc",
                65_000,
                3_250,
                61_750,
                1_900,
            ),
            line(
                by_lang(lang, "Tomates", "طماطم", "Tomatoes"),
                1_250,
                "kg",
                12_000,
                0,
                15_000,
                900,
            ),
        ],
        item_count: Qty(4_250),
        show_tax: true,
        ifu_mention: false,
        subtotal_ht: Money(93_175),
        taxes: vec![
            ReceiptTax {
                rate_bp: Rate(1_900),
                base_ht: Money(51_891),
                tax: Money(9_859),
            },
            ReceiptTax {
                rate_bp: Rate(900),
                base_ht: Money(41_284),
                tax: Money(3_716),
            },
        ],
        discount_total: Money(3_250),
        total_ttc: Money(106_750),
        stamp_duty: Money(1_100),
        stamp_absorbed: Money(0),
        rounding: Money(0),
        total_due: Money(107_850),
        payments: vec![cash(120_000)],
        change: Money(12_150),
        total_in_letters: by_lang(
            lang,
            "mille soixante-dix-huit dinars et cinquante centimes",
            "ألف وثمانية وسبعون ديناراً وخمسون سنتيماً",
            "one thousand seventy-eight dinars and fifty centimes",
        ),
        original_number: None,
        note: None,
        footer_text: Some(by_lang(
            lang,
            "Ouvert 7j/7 de 7h à 22h",
            "مفتوح 7 أيام من 7 إلى 22",
            "Open every day 7am-10pm",
        )),
        duplicate: false,
        barcode: "TK2026-C1-000123".into(),
        store_url: None,
        customer_balance: None,
        customer: None,
        dining: None,
        loyalty: None,
    }
}

pub(crate) fn z_report() -> ZReport {
    let session = CashSession {
        id: "s-1".into(),
        terminal_id: "t-1".into(),
        user_id: "u-2".into(),
        business_date: "2026-09-28".into(),
        opened_at: 1_790_568_000_000,
        opening_float: Money(1_000_000),
        closed_at: Some(1_790_611_200_000),
        closed_by: Some("u-2".into()),
        expected_cash: Some(Money(7_938_250)),
        counted_cash: Some(Money(7_937_500)),
        variance: Some(Money(-750)),
        status: SessionStatus::Closed,
        z_number: Some("ZC1-000031".into()),
        notes: None,
    };
    let summary = SessionSummary {
        session,
        terminal_code: "C1".into(),
        cashier_name: "Yacine".into(),
        tickets: 42,
        returns: 1,
        first_ticket: Some("TK2026-C1-000101".into()),
        last_ticket: Some("TK2026-C1-000142".into()),
        sales_ttc: Money(9_850_000),
        returns_ttc: Money(61_750),
        invoices: 0,
        invoices_ttc: Money::ZERO,
        credit_notes: 0,
        credit_notes_ttc: Money::ZERO,
        net_ttc: Money(9_788_250),
        discount_total: Money(12_000),
        per_rate: vec![RateTotal {
            rate_bp: Rate(1_900),
            base_ht: Money(5_000_000),
            tax: Money(950_000),
            total_ttc: Money(5_950_000),
        }],
        stamp_charged: Money(52_300),
        stamp_absorbed: Money(0),
        rounding: Money(0),
        by_method: vec![
            MethodTotal {
                method: PaymentMethod::Cash,
                received: Money(7_000_000),
                paid_back: Money(61_750),
                net: Money(6_938_250),
                count: 38,
            },
            MethodTotal {
                method: PaymentMethod::CardCib,
                received: Money(2_850_000),
                paid_back: Money(0),
                net: Money(2_850_000),
                count: 4,
            },
        ],
        opening_float: Money(1_000_000),
        cash_sales: Money(7_000_000),
        cash_refunds: Money(61_750),
        paid_in: Money(0),
        paid_out: Money(0),
        credit_payments: Money(0),
        drops: Money(0),
        expenses: vec![ExpenseTotal {
            category: "transport".into(),
            amount: Money(0),
        }],
        expenses_total: Money(0),
        expected_cash: Some(Money(7_938_250)),
        blind: false,
        show_tax: true,
        ifu_mention: false,
    };
    ZReport {
        z_number: "ZC1-000031".into(),
        z_seq: 31,
        summary,
        counts: vec![
            CountLine {
                denomination: Money(200_000),
                count: 35,
            },
            CountLine {
                denomination: Money(100_000),
                count: 9,
            },
            CountLine {
                denomination: Money(50_000),
                count: 1,
            },
            CountLine {
                denomination: Money(20_000),
                count: 1,
            },
            CountLine {
                denomination: Money(10_000),
                count: 1,
            },
            CountLine {
                denomination: Money(5_000),
                count: 1,
            },
            CountLine {
                denomination: Money(2_000),
                count: 1,
            },
            CountLine {
                denomination: Money(500),
                count: 1,
            },
        ],
        counted_cash: Money(7_937_500),
        variance: Money(-750),
        grand_total: Money(412_345_600),
        grand_total_carried: None,
        audit_anchor: ChainAnchor {
            last_seq: 9_876,
            last_hash: "ab".repeat(32),
            intact: true,
        },
        document_anchors: vec![DocAnchor {
            doc_type: DocType::Ticket,
            series: "C1".into(),
            last_seq: 142,
            last_hash: "cd".repeat(32),
            intact: true,
        }],
        closed_at: 1_790_611_200_000,
        closed_by: "u-2".into(),
        closed_by_name: "Yacine".into(),
        approved_by: None,
        notes: None,
        prev_hash: "0".repeat(64),
        hash: "ef".repeat(32),
        offline_pending: None,
    }
}

/// A 64×32 PNG: a black square in a white frame.
pub(crate) fn logo_png() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, 64, 32);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let data: Vec<u8> = (0..32)
            .flat_map(|y| {
                (0..64).map(move |x| {
                    if (8..56).contains(&x) && (4..28).contains(&y) {
                        0
                    } else {
                        255
                    }
                })
            })
            .collect();
        writer.write_image_data(&data).unwrap();
    }
    out
}

/// The built-in `id` template in `lang`.
pub(crate) fn template(id: &str, lang: LabelLang) -> LabelTemplate {
    let mut t = builtin_templates()
        .into_iter()
        .find(|t| t.id == id)
        .unwrap();
    t.lang = lang;
    t
}

fn blank(lang: LabelLang, name: &str) -> LabelData {
    LabelData {
        product_id: Some("p-1".into()),
        copies: 1,
        lang,
        name: name.into(),
        name_second: None,
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
        barcode: None,
        demo: false,
        jewel: None,
    }
}

/// Candia milk on the 40×25 price label: 150 DA, EAN-13.
pub(crate) fn milk_label(lang: LabelLang) -> (LabelTemplate, LabelData) {
    let name = if lang.primary() == Lang::Ar {
        "حليب كانديا 1 لتر"
    } else {
        "Lait Candia 1 L"
    };
    let label = LabelData {
        copies: 3,
        name_second: lang.second().map(|_| "Lait Candia 1 L".to_owned()),
        price: Money(15_000),
        barcode: Some(LabelBarcode {
            symbology: LabelSymbology::Ean13,
            data: "6130123456782".into(),
        }),
        ..blank(lang, name)
    };
    (template("builtin.price_40x25", lang), label)
}

/// Elio oil on the 58×40 shelf label, Arabic + French, 535,50 DA instead of 595 DA.
pub(crate) fn oil_shelf_label() -> (LabelTemplate, LabelData) {
    let label = LabelData {
        name_second: Some("Huile Elio 5 L".into()),
        price: Money(53_550),
        old_price: Some(Money(59_500)),
        shop_name: Some("سوبيرات الأمل".into()),
        barcode: Some(LabelBarcode {
            symbology: LabelSymbology::Ean13,
            data: "6130123456782".into(),
        }),
        ..blank(LabelLang::ArFr, "زيت إليو 5 لتر")
    };
    (template("builtin.shelf_58x40", LabelLang::ArFr), label)
}

/// 1.25 kg of tomatoes at 120 DA/kg on the weighed label, in Arabic. `2110012012507`: prefix 21,
/// PLU 10012, 1 250 g, GS1 check digit 7.
pub(crate) fn tomato_weighed_label() -> (LabelTemplate, LabelData) {
    let label = LabelData {
        price: Money(15_000),
        unit_code: "kg".into(),
        unit_price: Some(Money(12_000)),
        weight: Some(Qty(1_250)),
        packed_on: Some("2026-09-28".into()),
        expiry: Some("2026-10-01".into()),
        shop_name: Some("سوبيرات الأمل".into()),
        barcode: Some(LabelBarcode {
            symbology: LabelSymbology::Ean13,
            data: "2110012012507".into(),
        }),
        ..blank(LabelLang::Ar, "طماطم")
    };
    (template(WEIGHED_TEMPLATE, LabelLang::Ar), label)
}

/// A receipt printer that appends to `dir/file`, with M1b's default 80 mm profile.
pub(crate) fn file_printer(dir: &std::path::Path, id: &str, file: &str) -> PrinterConfig {
    PrinterConfig {
        id: id.into(),
        name: id.into(),
        transport: Transport::File {
            path: dir.join(file).to_string_lossy().into_owned(),
        },
        profile: Profile::preset(Preset::Generic, PaperWidth::Mm80),
        mode: PrintMode::Raw,
        paper_width_mm: 80,
    }
}

/// Table T4 at the grill station, 3 covers, waiter Sofiane, at 12:00: two skewers with fries and
/// a note on seat 1, three chorbas for the second course. Its words come from `pos-docs`.
fn kitchen_ticket(lang: Lang, cancelled: bool) -> KitchenTicket {
    KitchenTicket {
        id: "k1".into(),
        ticket_id: "t1".into(),
        station_id: Some("st-grill".into()),
        station_name: Some(by_lang(lang, "Grill", "الشواء", "Grill")),
        seq: 2,
        kind: if cancelled {
            KitchenKind::Cancel
        } else {
            KitchenKind::Order
        },
        lines: vec![
            KitchenLine {
                name: by_lang(lang, "Brochettes", "شواء مشكل", "Skewers"),
                qty: Qty(2_000),
                unit_code: "pc".into(),
                note: Some(by_lang(lang, "Bien cuit", "مطهو جيداً", "Well done")),
                seat: Some(1),
                course: None,
                options: vec![by_lang(lang, "Frites", "بطاطا مقلية", "Fries")],
            },
            KitchenLine {
                name: by_lang(lang, "Chorba frik", "شربة فريك", "Chorba frik"),
                qty: Qty(3_000),
                unit_code: "pc".into(),
                note: None,
                seat: None,
                course: Some(2),
                options: Vec::new(),
            },
        ],
        mode: DiningMode::DineIn,
        order_no: None,
        table_code: Some("T4".into()),
        label: None,
        covers: 3,
        waiter_name: "Sofiane".into(),
        created_at: 1_790_593_200_000,
        printed_at: None,
    }
}

/// The kitchen ticket of table T4 (`cancelled`: its cancellation ticket) in `lang`.
pub(crate) fn fixture_kitchen_slip(lang: Lang, cancelled: bool) -> pos_docs::slips::KitchenSlip {
    kitchen_slip(&kitchen_ticket(lang, cancelled), lang)
}

/// Takeaway order `n`: no table, no station (the kitchen fallback), no covers.
pub(crate) fn fixture_takeaway_slip(lang: Lang, n: i64) -> pos_docs::slips::KitchenSlip {
    let ticket = KitchenTicket {
        station_id: None,
        station_name: None,
        mode: DiningMode::Takeaway,
        order_no: Some(n),
        table_code: None,
        covers: 0,
        ..kitchen_ticket(lang, false)
    };
    kitchen_slip(&ticket, lang)
}

/// The addition of table T4: 1 200 DA of skewers and 450 DA of chorba, 1 650 DA, 1 667 DA in
/// cash with the duty the bill computed, shared in three.
pub(crate) fn fixture_prebill(lang: Lang) -> PreBillSlip {
    let l = |key: &str| pos_docs::receipt::label(lang, key);
    PreBillSlip {
        lang,
        rtl: lang.is_rtl(),
        shop: by_lang(
            lang,
            "Restaurant El Bahdja",
            "مطعم البهجة",
            "El Bahdja Restaurant",
        ),
        title: l("slip.prebill.title"),
        place: l("slip.table").replace("{table}", "T4"),
        covers: l("slip.covers").replace("{n}", "3"),
        waiter: "Sofiane".into(),
        lines: vec![
            PreBillLine {
                qty: "2".into(),
                name: by_lang(lang, "Brochettes", "شواء مشكل", "Skewers"),
                total: Money(120_000),
            },
            PreBillLine {
                qty: "3".into(),
                name: by_lang(lang, "Chorba frik", "شربة فريك", "Chorba frik"),
                total: Money(45_000),
            },
        ],
        total_label: l("slip.prebill.total"),
        total: Money(165_000),
        cash_label: l("slip.prebill.cash"),
        cash_due: Money(166_700),
        shares_label: Some(l("slip.prebill.shares").replace("{n}", "3")),
        shares: vec![Money(55_000), Money(55_000), Money(55_000)],
        notice: l("slip.prebill.notice"),
    }
}

/// The Task 13 sample: the 5 000 DA gift voucher `BA2026-000001`, barcode `2982600000018`
/// (`298` + `26` + `0000001` + check digit 8), sold for cash.
pub(crate) fn fixture_voucher_slip(lang: Lang) -> VoucherSlip {
    pos_docs::slips::voucher_slip(
        &pos_core::gift_vouchers::Voucher::sample(),
        &SlipShop::sample(),
        lang,
    )
}
