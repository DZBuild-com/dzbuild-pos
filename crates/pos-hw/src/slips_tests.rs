//! Pack slips on thermal paper (Task 21 the voucher; Task 18 the others and kitchen routing).

use crate::config::{HardwareConfig, KitchenRouting, PrintMode, StationPrinter};
use crate::driver::Transport;
use crate::error::HwError;
use crate::layout::{Align, Layout, Row};
use crate::receipt::receipt_layout;
use crate::registry::Registry;
use crate::slips::{
    deposit_layout, intake_layout, kitchen_layout, prebill_layout, voucher_layout, with_demo,
};
use crate::testdata::{
    file_printer, fixture_kitchen_slip, fixture_prebill, fixture_takeaway_slip,
    fixture_voucher_slip, ticket,
};
use pos_core::lang::Lang;
use pos_core::money::Money;
use pos_docs::slips::{SlipShop, VoucherSlip, VoucherSlipDuty};

/// The Task 13 sample: a 5 000 DA gift voucher sold for cash, 50 DA of duty charged.
fn voucher(lang: Lang) -> VoucherSlip {
    VoucherSlip {
        lang,
        rtl: lang.is_rtl(),
        title: "Bon cadeau".into(),
        code: "BA2026-000001".into(),
        barcode: "2982600000018".into(),
        amount: Money(500_000),
        expires_on: "2027-09-28".into(),
        conditions: "Valable jusqu'au 2027-09-28, non remboursable.".into(),
        shop: SlipShop::sample(),
        duty: Some(VoucherSlipDuty {
            stamp_label: "Droit de timbre".into(),
            stamp_duty: Money(5_000),
            received_label: "Reçu".into(),
            received: Money(505_000),
        }),
    }
}

#[test]
fn a_voucher_prints_its_amount_code_duty_and_barcode() {
    let layout = voucher_layout(&voucher(Lang::Fr));
    assert!(!layout.rtl);
    let texts: Vec<&str> = layout.texts().collect();
    assert!(texts.contains(&"Supérette El Amel"), "{texts:?}");
    assert!(texts.contains(&"Bon cadeau"));
    assert!(texts.contains(&"BA2026-000001"));
    assert!(texts.contains(&"5 000,00 DA"), "{texts:?}");
    assert!(texts.contains(&"Droit de timbre"));
    assert!(texts.iter().any(|t| t.contains("2027-09-28")));
    assert!(
        layout
            .rows
            .iter()
            .any(|r| matches!(r, Row::Barcode { data } if data == "2982600000018"))
    );
    let amount_row = layout
        .rows
        .iter()
        .find(|r| matches!(r, Row::Text { text, .. } if text.contains("000,00")))
        .unwrap();
    assert!(
        matches!(
            amount_row,
            Row::Text {
                large: true,
                bold: true,
                ..
            }
        ),
        "the amount is the loud line"
    );
    assert!(voucher_layout(&voucher(Lang::Ar)).rtl);
    let card = VoucherSlip {
        duty: None,
        ..voucher(Lang::Fr)
    };
    assert!(
        !voucher_layout(&card)
            .texts()
            .any(|t| t == "Droit de timbre"),
        "no duty line without duty"
    );
}

#[test]
fn kitchen_tickets_and_slips_lay_out_in_each_language() {
    for (name, slip) in [
        (
            "kitchen_grill_fr",
            fixture_kitchen_slip(pos_core::lang::Lang::Fr, false),
        ),
        (
            "kitchen_cancel_ar",
            fixture_kitchen_slip(pos_core::lang::Lang::Ar, true),
        ),
        (
            "kitchen_takeaway_en",
            fixture_takeaway_slip(pos_core::lang::Lang::En, 42),
        ),
    ] {
        insta::assert_json_snapshot!(name, kitchen_layout(&slip));
    }
    insta::assert_json_snapshot!(
        "prebill_fr",
        prebill_layout(&fixture_prebill(pos_core::lang::Lang::Fr))
    );
    let voucher = voucher_layout(&fixture_voucher_slip(pos_core::lang::Lang::Fr));
    assert!(voucher.texts().any(|t| t.contains("BA2026-000001")));
    assert!(
        voucher
            .rows
            .iter()
            .any(|r| matches!(r, crate::layout::Row::Barcode { data } if data == "2982600000018"))
    );
}

#[test]
fn kitchen_tickets_go_to_their_station_printer_with_copies_and_the_buzzer() {
    let dir = tempfile::tempdir().unwrap();
    let registry = Registry::open(dir.path().join("hardware.json"));
    let grill = file_printer(dir.path(), "grill", "grill.prn");
    let bar = file_printer(dir.path(), "bar", "bar.prn");
    let receipt = file_printer(dir.path(), "caisse", "caisse.prn");
    registry
        .save(HardwareConfig {
            printers: vec![grill.clone(), bar.clone(), receipt.clone()],
            receipt_printer_id: Some(receipt.id.clone()),
            kitchen: KitchenRouting {
                stations: vec![StationPrinter {
                    station_id: "st-grill".into(),
                    printer_id: grill.id.clone(),
                    copies: 2,
                    buzzer: true,
                }],
                fallback_printer_id: Some(bar.id.clone()),
            },
            ..HardwareConfig::default()
        })
        .unwrap();
    let layout = kitchen_layout(&fixture_kitchen_slip(pos_core::lang::Lang::Fr, false));
    let sent = registry.print_kitchen(Some("st-grill"), &layout).unwrap();
    assert_eq!(sent.len(), 2);
    let bytes = std::fs::read(dir.path().join("grill.prn")).unwrap();
    assert!(
        bytes.windows(2).filter(|w| *w == [0x1B, b'B']).count() == 2,
        "the buzzer sounds after each copy"
    );
    registry.print_kitchen(Some("st-pizza"), &layout).unwrap();
    assert!(
        dir.path().join("bar.prn").exists(),
        "an unmapped station prints on the kitchen fallback"
    );
    // the buzzer and the copies are the station's: the fallback prints once, silently
    let fallback = std::fs::read(dir.path().join("bar.prn")).unwrap();
    assert!(!fallback.windows(2).any(|w| w == [0x1B, b'B']));
    assert_eq!(
        fallback.windows(2).filter(|w| *w == [0x1B, b'@']).count(),
        1
    );
    // the cut comes before the buzzer: the cook tears the ticket while it rings
    let cut = bytes.windows(2).position(|w| w == [0x1D, b'V']).unwrap();
    let buzz = bytes.windows(2).position(|w| w == [0x1B, b'B']).unwrap();
    assert!(cut < buzz);
    assert_eq!(&bytes[buzz..buzz + 4], &crate::escpos::buzzer(3, 2));

    // no kitchen fallback: the till's receipt printer
    let mut config = registry.config().unwrap();
    config.kitchen.fallback_printer_id = None;
    registry.save(config).unwrap();
    registry.print_kitchen(None, &layout).unwrap();
    assert!(dir.path().join("caisse.prn").exists());
    // nothing at all: not configured
    let mut config = registry.config().unwrap();
    config.receipt_printer_id = None;
    registry.save(config).unwrap();
    assert_eq!(
        registry
            .print_kitchen(Some("st-pizza"), &layout)
            .unwrap_err(),
        HwError::NotConfigured
    );
    assert_eq!(
        registry
            .print_kitchen(Some("st-grill"), &layout)
            .unwrap()
            .len(),
        2,
        "a mapped station needs no receipt printer"
    );
}

#[test]
fn a_driver_only_printer_asks_for_a_page() {
    let dir = tempfile::tempdir().unwrap();
    let registry = Registry::open(dir.path().join("hardware.json"));
    let mut gdi = file_printer(dir.path(), "gdi", "gdi.prn");
    gdi.transport = Transport::Windows {
        printer_name: "HP LaserJet".into(),
    };
    gdi.mode = PrintMode::Driver;
    gdi.paper_width_mm = 80;
    registry
        .save(HardwareConfig {
            printers: vec![gdi.clone()],
            receipt_printer_id: Some(gdi.id.clone()),
            ..HardwareConfig::default()
        })
        .unwrap();
    let err = registry
        .print_to(
            &gdi.id,
            &prebill_layout(&fixture_prebill(pos_core::lang::Lang::Fr)),
        )
        .unwrap_err();
    assert!(
        matches!(err, HwError::DriverOnly { ref printer_name, paper_width_mm: 80 } if printer_name == "HP LaserJet")
    );
    assert_eq!(err.code(), "printer_driver_only");
    // the receipt, the reports and the kitchen tickets never reach it as ESC/POS either
    let page = |e: HwError| matches!(e, HwError::DriverOnly { .. });
    assert!(page(
        registry
            .print_receipt(&ticket(Lang::Fr), None, false)
            .unwrap_err()
    ));
    assert!(page(
        registry
            .receipt_bytes(&ticket(Lang::Fr), None, false)
            .unwrap_err()
    ));
    let layout = prebill_layout(&fixture_prebill(Lang::Fr));
    assert!(page(registry.print_layout(&layout).unwrap_err()));
    assert!(page(registry.print_kitchen(None, &layout).unwrap_err()));
    assert!(page(registry.test_page(&gdi, Lang::Fr).unwrap_err()));
}

fn texts(layout: &Layout) -> Vec<&str> {
    layout.texts().collect()
}

/// The row that says `text` exactly.
fn row<'a>(layout: &'a Layout, text: &str) -> &'a Row {
    layout
        .rows
        .iter()
        .find(|r| matches!(r, Row::Text { text: t, .. } if t == text))
        .unwrap_or_else(|| panic!("no row {text:?} in {:?}", texts(layout)))
}

fn position(layout: &Layout, text: &str) -> usize {
    layout
        .rows
        .iter()
        .position(|r| match r {
            Row::Text { text: t, .. } => t == text,
            Row::Pair { start, .. } => start == text,
            _ => false,
        })
        .unwrap_or_else(|| panic!("no row {text:?} in {:?}", texts(layout)))
}

fn large(row: &Row) -> bool {
    matches!(row, Row::Text { large: true, .. })
}

#[test]
fn a_kitchen_ticket_prints_each_line_large_with_its_options_note_seat_and_course() {
    let fr = kitchen_layout(&fixture_kitchen_slip(Lang::Fr, false));
    assert!(!fr.rtl);
    assert!(large(row(&fr, "Bon de cuisine")));
    assert!(large(row(&fr, "Grill")));
    assert!(large(row(&fr, "Table T4")));
    assert!(large(row(&fr, "2 × Brochettes")));
    assert!(large(row(&fr, "3 × Chorba frik")));
    for detail in ["   + Frites", "   ! Bien cuit", "   Place 1", "   Suite 2"] {
        assert!(!large(row(&fr, detail)), "{detail}");
    }
    let brochettes = position(&fr, "2 × Brochettes");
    assert!(brochettes < position(&fr, "   + Frites"));
    assert!(position(&fr, "   ! Bien cuit") < position(&fr, "3 × Chorba frik"));
    assert!(
        fr.rows
            .iter()
            .any(|r| matches!(r, Row::Pair { start, end, .. }
            if start == "Sofiane — 3 couverts" && end == "12:00 · Envoi 2")),
        "{:?}",
        texts(&fr)
    );

    let cancel = kitchen_layout(&fixture_kitchen_slip(Lang::Ar, true));
    assert!(cancel.rtl);
    assert!(
        matches!(&cancel.rows[0], Row::Text { text, align: Align::Center, large: true, bold: true } if text == "*** إلغاء ***"),
        "the cancellation says so first: {:?}",
        cancel.rows[0]
    );

    let takeaway = kitchen_layout(&fixture_takeaway_slip(Lang::En, 42));
    assert!(large(row(&takeaway, "Takeaway no. 42")));
    assert!(
        !texts(&takeaway).iter().any(|t| t.contains("covers")),
        "no covers for a takeaway: {:?}",
        texts(&takeaway)
    );
    assert!(!texts(&takeaway).contains(&"Grill"), "no station");
}

#[test]
fn the_addition_shows_its_total_the_shares_and_says_it_is_not_a_receipt() {
    let slip = fixture_prebill(Lang::Fr);
    let layout = prebill_layout(&slip);
    assert!(large(row(&layout, "Restaurant El Bahdja")));
    assert!(large(row(&layout, "Addition")));
    let all = texts(&layout);
    for text in [
        "Table T4 — 3 couverts",
        "2 × Brochettes",
        "1 200,00",
        "Total TTC",
        "1 650,00 DA",
        "À payer en espèces (timbre compris)",
        "1 667,00 DA",
        "Partagé en 3 :",
        "550,00",
    ] {
        assert!(all.contains(&text), "{text} in {all:?}");
    }
    assert!(
        matches!(layout.rows.iter().rev().find(|r| matches!(r, Row::Text { .. })),
            Some(Row::Text { text, bold: true, .. }) if text == "Ce document n'est pas un ticket de caisse."),
        "the notice closes the addition"
    );
    let no_shares = prebill_layout(&pos_docs::slips::PreBillSlip {
        shares_label: None,
        shares: Vec::new(),
        ..slip
    });
    assert!(!texts(&no_shares).contains(&"Partagé en 3 :"));
    assert!(prebill_layout(&fixture_prebill(Lang::Ar)).rtl);
}

#[test]
fn the_repair_intake_and_the_deposit_receipt_carry_their_numbers_conditions_and_duty() {
    let intake = intake_layout(&pos_docs::slips::intake_slip(
        &pos_core::service_tickets::tests_fixture(),
        "Phones El Amel",
        Some("Tout appareil non retiré après 90 jours…"),
        Lang::Fr,
        1_790_593_200_000,
    ));
    assert!(large(row(&intake, "Bon de réparation")));
    assert!(large(row(&intake, "RP2026-000001")));
    let all = texts(&intake);
    for text in [
        "Nassim Ould Ali",
        "IMEI",
        "356938035643809",
        "Panne / travail",
        "Remplacement écran",
        "Devis",
        "15 000,00 DA",
        "Prêt le",
        "01/10/2026",
        "Tout appareil non retiré après 90 jours…",
        "Signature du client",
    ] {
        assert!(all.iter().any(|t| t.contains(text)), "{text} in {all:?}");
    }
    assert!(
        intake
            .rows
            .iter()
            .any(|r| matches!(r, Row::Barcode { data } if data == "RP2026-000001")),
        "the counter scans the ticket number"
    );

    use pos_core::deposits::{Deposit, DepositKind, OwnerType};
    let deposit = Deposit {
        id: "d1".into(),
        owner_type: OwnerType::ServiceTicket,
        owner_id: "t1".into(),
        customer_id: None,
        kind: DepositKind::Deposit,
        amount: Money(500_000),
        method: Some(pos_core::payments::PaymentMethod::Cash),
        reference: None,
        stamp_duty: Money(5_000),
        stamp_charged: Money(5_000),
        receipt_number: Some("AC2026-000001".into()),
        document_id: None,
        business_date: "2026-09-28".into(),
        created_at: 1_790_593_200_000,
    };
    let receipt = deposit_layout(
        &pos_docs::slips::deposit_slip(
            &deposit,
            "RP2026-000001",
            Some("Nassim Ould Ali"),
            Money(700_000),
            Lang::Fr,
        ),
        "Phones El Amel",
        "28/09/2026 10:00",
    );
    assert!(large(row(&receipt, "Phones El Amel")), "the shop heads it");
    assert!(large(row(&receipt, "Reçu d'arrhes")));
    assert!(large(row(&receipt, "AC2026-000001")));
    let all = texts(&receipt);
    for text in [
        "28/09/2026 10:00",
        "Pour RP2026-000001",
        "Nassim Ould Ali",
        "5 000,00 DA",
        "Droit de timbre",
        "50,00 DA",
        "Total reçu",
        "5 050,00 DA",
        "Arrhes versées à ce jour",
        "7 000,00 DA",
    ] {
        assert!(all.contains(&text), "{text} in {all:?}");
    }
}

/// A slip printed from the demo shop says "DÉMO – sans valeur fiscale" first, like its
/// receipts.
#[test]
fn a_demo_slip_says_so_first() {
    let layout = kitchen_layout(&fixture_kitchen_slip(Lang::Fr, false));
    let marked = with_demo(layout.clone(), Lang::Fr, true);
    assert_eq!(
        marked.rows[0],
        Row::Text {
            text: "DÉMO – sans valeur fiscale".into(),
            align: Align::Center,
            large: true,
            bold: true,
        }
    );
    assert_eq!(&marked.rows[1..], &layout.rows[..]);
    assert_eq!(with_demo(layout.clone(), Lang::Fr, false), layout);
    let ar = with_demo(
        voucher_layout(&fixture_voucher_slip(Lang::Ar)),
        Lang::Ar,
        true,
    );
    assert!(matches!(&ar.rows[0], Row::Text { text, .. } if text == "تجريبي – بدون قيمة جبائية"));
}

/// RST-009: the place of a restaurant sale above the lines (a takeaway's number in large type)
/// and LOY-002: the points it earned under the totals.
#[test]
fn a_restaurant_receipt_prints_its_place_above_the_lines_and_its_points_under_the_totals() {
    use pos_core::restaurant::bill::DiningInfo;
    use pos_core::restaurant::tickets::DiningMode;
    let takeaway = DiningInfo {
        mode: DiningMode::Takeaway,
        order_no: Some(42),
        table_code: None,
        label: None,
        covers: 0,
    };
    let mut receipt = ticket(Lang::Fr).with_loyalty(pos_docs::receipt::ReceiptLoyalty {
        earned: 23,
        balance: 36,
        card: "2970000000015".into(),
    });
    receipt.dining = Some(pos_docs::receipt::dining(&takeaway, Lang::Fr));
    let layout = receipt_layout(&receipt, None);
    let first_line = position(&layout, "Lait Candia 1 L");
    assert!(position(&layout, "À emporter") < first_line);
    let number = position(&layout, "Commande n° 42");
    assert!(number < first_line && large(&layout.rows[number]));
    let points = position(&layout, "Points fidélité : +23 — solde 36");
    assert!(points > position(&layout, "Net à payer"));

    let table = DiningInfo {
        mode: DiningMode::DineIn,
        order_no: None,
        table_code: Some("T4".into()),
        label: None,
        covers: 3,
    };
    receipt.dining = Some(pos_docs::receipt::dining(&table, Lang::Fr));
    let layout = receipt_layout(&receipt, None);
    assert!(position(&layout, "Sur place") < position(&layout, "Table T4 — 3 couverts"));
    assert!(position(&layout, "Table T4 — 3 couverts") < position(&layout, "Lait Candia 1 L"));

    let plain = receipt_layout(&ticket(Lang::Fr), None);
    assert!(
        !texts(&plain)
            .iter()
            .any(|t| t.contains("Points fidélité") || t.contains("Sur place")),
        "a shop ticket prints neither"
    );
}
