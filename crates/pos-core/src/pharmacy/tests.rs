#![allow(
    clippy::needless_update,
    reason = "`SaleExtras` gains a field per pack (serials, buyer, payment types, location)"
)]

use super::chifa::{self, ChifaMapping};
use super::register::PrescriptionInput;
use super::*;
use crate::auth::Permission;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::payments::PaymentInput;
use crate::sales::{Cart, CartLine, SaleExtras, SaleInput, create_sale_with};
use crate::testkit::{NOON, Shop, shop};

pub(crate) fn pharmacy() -> Shop {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| {
            packs::set_modules(tx, &actor, &[Module::Pharmacy, Module::Lots, Module::Staff])
        })
        .unwrap();
    shop
}

pub(crate) fn medicine(shop: &Shop, name: &str, price: i64, dci: &str, controlled: bool) -> String {
    let id = shop.product(name, Money(price), &shop.exempt).id;
    let (actor, pid) = (shop.owner.actor(), id.clone());
    let input = PharmacyProductInput {
        dci: Some(dci.into()),
        controlled,
        schedule: controlled.then(|| "Psychotrope".to_owned()),
        prescription_required: controlled,
    };
    shop.db
        .write(move |tx| set(tx, &actor, &pid, input))
        .unwrap();
    id
}

pub(crate) fn prescription(line: u32) -> PrescriptionInput {
    PrescriptionInput {
        line,
        prescriber_name: "Dr Meriem Haddad".into(),
        prescriber_ref: Some("CNOM 16-0452".into()),
        prescription_no: Some("ORD-7781".into()),
        prescription_date: "2026-09-27".into(),
        patient_name: "Rachid B.".into(),
        patient_ref: None,
    }
}

pub(crate) fn sell(
    shop: &Shop,
    ctx: crate::till::TillContext,
    lines: Vec<CartLine>,
    extras: SaleExtras,
) -> Result<crate::sales::ValidatedSale, crate::error::AppError> {
    let input = SaleInput {
        cart: Cart::new(Lang::Fr, lines),
        payments: vec![PaymentInput::cash(Money(1_000_000))],
    };
    shop.db
        .write(move |tx| create_sale_with(tx, &ctx, input, &extras))
}

#[test]
fn dci_makes_a_medicine_findable_and_lists_its_generics() {
    let shop = pharmacy();
    let doliprane = medicine(&shop, "Doliprane 1000", 30_000, "Paracétamol", false);
    let generic = medicine(&shop, "Paracétamol Saidal 1g", 18_000, "PARACETAMOL", false);
    shop.stock_in(&generic, 20_000);
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM products WHERE search_text LIKE '%paracetamol%'"),
        2,
        "the DCI is searchable (SAL-011)"
    );
    let id = doliprane.clone();
    let found = shop
        .db
        .read(move |c| equivalents(c, &id, Lang::Fr, crate::catalog::RETAIL_LEVEL_ID))
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        (
            found[0].product_id.as_str(),
            found[0].price,
            found[0].on_hand,
            found[0].cheaper
        ),
        (generic.as_str(), Money(18_000), Qty(20_000), true)
    );
}

#[test]
fn the_register_needs_the_pharmacy_module_and_its_permission() {
    let shop = pharmacy();
    let lexo = medicine(&shop, "Lexomil 6 mg", 45_000, "Bromazépam", true);
    shop.open_till(100_000);
    let missing = sell(
        &shop,
        shop.till(),
        vec![CartLine::product(&lexo, Qty(1_000))],
        SaleExtras::default(),
    )
    .unwrap_err();
    assert_eq!(missing.code(), "register_entry_required");
    let extras = SaleExtras {
        prescriptions: vec![prescription(0)],
        ..SaleExtras::default()
    };
    let cashier = sell(
        &shop,
        shop.till(),
        vec![CartLine::product(&lexo, Qty(1_000))],
        extras.clone(),
    )
    .unwrap_err();
    assert_eq!(
        cashier.code(),
        "insufficient_permission",
        "health data needs pharmacy.register"
    );
    let ctx = shop.approved_till(Permission::PharmacyRegister);
    let sale = sell(
        &shop,
        ctx,
        vec![CartLine::product(&lexo, Qty(1_000))],
        extras.clone(),
    )
    .unwrap();
    let entries = shop
        .db
        .read(|c| register::entries(c, "2026-01-01", "2026-12-31"))
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        (
            entries[0].number.as_str(),
            entries[0].document_id.as_str(),
            entries[0].patient_name.as_str()
        ),
        ("2026-000001", sale.document.id.as_str(), "Rachid B.")
    );
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Staff]))
        .unwrap();
    let off = sell(
        &shop,
        shop.owner_till(),
        vec![CartLine::product(&lexo, Qty(1_000))],
        extras,
    )
    .unwrap_err();
    assert_eq!(
        off.code(),
        "module_disabled",
        "no health data outside the pharmacy pack"
    );
}

#[test]
fn the_register_is_numbered_per_year_and_never_edited() {
    let shop = pharmacy();
    let tramadol = medicine(&shop, "Tramadol 50", 25_000, "Tramadol", true);
    let plain = shop
        .product("Sérum physiologique", Money(5_000), &shop.exempt)
        .id;
    shop.open_till(100_000);
    for _ in 0..2 {
        let extras = SaleExtras {
            prescriptions: vec![prescription(1)],
            ..SaleExtras::default()
        };
        sell(
            &shop,
            shop.owner_till(),
            vec![
                CartLine::product(&plain, Qty(1_000)),
                CartLine::product(&tramadol, Qty(2_000)),
            ],
            extras,
        )
        .unwrap();
    }
    let numbers: Vec<String> = shop
        .db
        .read(|c| register::entries(c, "2026-01-01", "2026-12-31"))
        .unwrap()
        .into_iter()
        .map(|e| e.number)
        .collect();
    assert_eq!(numbers, ["2026-000001", "2026-000002"]);
    assert!(
        shop.db
            .write(|tx| Ok(tx.execute_batch("UPDATE controlled_register SET patient_name = 'X'")?))
            .is_err()
    );
    let wrong = SaleExtras {
        prescriptions: vec![prescription(0)],
        ..SaleExtras::default()
    };
    let err = sell(
        &shop,
        shop.owner_till(),
        vec![CartLine::product(&plain, Qty(1_000))],
        wrong,
    )
    .unwrap_err();
    assert_eq!(
        (err.code(), err.payload().field.as_deref()),
        ("value_invalid".into(), Some("prescriptions.0.line")),
        "an entry names a controlled line"
    );
}

#[test]
fn the_journal_never_copies_health_data() {
    let shop = pharmacy();
    let lexo = medicine(&shop, "Lexomil 6 mg", 45_000, "Bromazépam", true);
    shop.open_till(100_000);
    let extras = SaleExtras {
        prescriptions: vec![prescription(0)],
        ..SaleExtras::default()
    };
    sell(
        &shop,
        shop.owner_till(),
        vec![CartLine::product(&lexo, Qty(1_000))],
        extras,
    )
    .unwrap();
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'pharmacy.register_recorded'"),
        1
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM audit_log WHERE payload LIKE '%Rachid%' OR payload LIKE '%Haddad%' OR payload LIKE '%ORD-7781%'"),
        0
    );
}

#[test]
fn the_chifa_list_is_checked_then_applied_and_matched_by_barcode() {
    let shop = pharmacy();
    let mut input = crate::catalog::products::ProductInput::simple(
        "Augmentin 1g",
        crate::catalog::UNIT_PIECE_ID,
        &shop.exempt,
        Money(95_000),
    );
    input.barcodes = vec![crate::catalog::products::Barcode {
        barcode: "6130000123456".into(),
        pack_qty: Qty(1_000),
        label: None,
    }];
    let augmentin = shop.product_with(input).id;
    let rows: Vec<Vec<String>> = vec![
        vec![
            "Code".into(),
            "Désignation".into(),
            "Tarif de référence".into(),
            "Remboursable".into(),
        ],
        vec![
            "6130000123456".into(),
            "AUGMENTIN 1G CP".into(),
            "812,50".into(),
            "oui".into(),
        ],
        vec![
            "6130000999999".into(),
            "AUTRE".into(),
            "120".into(),
            "non".into(),
        ],
        vec![
            "".into(),
            "SANS CODE".into(),
            "abc".into(),
            "peut-être".into(),
        ],
    ];
    let mapping = ChifaMapping {
        code: 0,
        name: 1,
        reference_price: 2,
        reimbursable: Some(3),
    };
    let (r, m) = (rows.clone(), mapping.clone());
    let checked = shop.db.read(move |c| chifa::check(c, &r[1..], &m)).unwrap();
    assert_eq!((checked.rows, checked.valid, checked.matched), (3, 2, 1));
    let columns: Vec<&str> = checked.problems.iter().map(|p| p.column.as_str()).collect();
    assert_eq!(columns, ["code", "referencePrice", "reimbursable"]);
    let (actor, r, m) = (shop.owner.actor(), rows.clone(), mapping.clone());
    let refused = shop
        .db
        .write(move |tx| chifa::apply(tx, &actor, &r[1..], &m, "chifa.xlsx"))
        .unwrap_err();
    assert_eq!(refused.code(), "chifa_list_invalid");
    let (actor, m) = (shop.owner.actor(), mapping.clone());
    let good = rows[1..3].to_vec();
    let applied = shop
        .db
        .write(move |tx| chifa::apply(tx, &actor, &good, &m, "chifa.xlsx"))
        .unwrap();
    assert_eq!((applied.created, applied.matched), (2, 1));
    let price = shop
        .db
        .read(move |c| chifa::of_product(c, &augmentin))
        .unwrap()
        .unwrap();
    assert_eq!(
        (price.reference_price, price.reimbursable),
        (Money(81_250), true)
    );
}

#[test]
fn the_expiry_board_splits_expired_and_soon_lots_with_their_cost() {
    let shop = pharmacy();
    let sirop = shop.product("Sirop toux", Money(40_000), &shop.exempt).id;
    let actor = shop.owner.actor();
    let pid = sirop.clone();
    shop.db
        .write(move |tx| {
            crate::catalog::extras::set_quiet(
                tx,
                &pid,
                crate::catalog::extras::ProductExtrasInput {
                    track_lots: true,
                    ..Default::default()
                },
            )?;
            for (lot, expiry, qty) in [
                ("L1", "2026-09-01", 2_000),
                ("L2", "2026-10-10", 3_000),
                ("L3", "2027-06-01", 5_000),
            ] {
                crate::stock::receive(
                    tx,
                    &actor,
                    &crate::stock::ReceiveLine {
                        product_id: pid.clone(),
                        location_id: crate::stock::SHOP_LOCATION_ID.into(),
                        qty: Qty(qty),
                        unit_cost: Money(20_000),
                        lot: Some(crate::stock::lots::LotInput {
                            lot_number: lot.into(),
                            expiry_date: Some(expiry.into()),
                        }),
                        serials: Vec::new(),
                        supplier_id: None,
                        ref_type: "test".into(),
                        ref_id: "t".into(),
                        note: None,
                    },
                    NOON,
                )?;
            }
            Ok(())
        })
        .unwrap();
    let board = shop.db.read(|c| expiry_board(c, "2026-09-28", 30)).unwrap();
    assert_eq!((board.expired.len(), board.soon.len()), (1, 1));
    assert_eq!(
        (board.expired_cost, board.soon_cost),
        (Money(40_000), Money(60_000))
    );
}

#[test]
fn a_controlled_line_takes_one_register_entry() {
    let shop = pharmacy();
    let lexo = medicine(&shop, "Lexomil 6 mg", 45_000, "Bromazépam", true);
    let id = lexo.clone();
    let fiche = shop.db.read(move |c| get(c, &id)).unwrap().unwrap();
    assert_eq!(
        (
            fiche.dci.as_deref(),
            fiche.controlled,
            fiche.schedule.as_deref()
        ),
        (Some("Bromazépam"), true, Some("Psychotrope"))
    );
    shop.open_till(100_000);
    let twice = SaleExtras {
        prescriptions: vec![prescription(0), prescription(0)],
        ..SaleExtras::default()
    };
    let err = sell(
        &shop,
        shop.owner_till(),
        vec![CartLine::product(&lexo, Qty(1_000))],
        twice,
    )
    .unwrap_err();
    assert_eq!(
        (err.code(), err.payload().field.as_deref()),
        ("value_invalid".into(), Some("prescriptions.1.line"))
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM documents"),
        0,
        "the refused ticket rolled back, its number included"
    );
    assert_eq!(shop.count("SELECT COUNT(*) FROM controlled_register"), 0);
}

#[test]
fn a_new_chifa_list_updates_the_codes_already_known() {
    let shop = pharmacy();
    let mapping = ChifaMapping {
        code: 0,
        name: 1,
        reference_price: 2,
        reimbursable: None,
    };
    let row = |code: &str, name: &str, price: &str| -> Vec<String> {
        vec![code.to_owned(), name.to_owned(), price.to_owned()]
    };
    let apply = |rows: Vec<Vec<String>>| {
        let (actor, m) = (shop.owner.actor(), mapping.clone());
        shop.db
            .write(move |tx| chifa::apply(tx, &actor, &rows, &m, "chifa.csv"))
    };
    apply(vec![
        row("A1", "AMOXICILLINE 500", "150"),
        row("B2", "IBUPROFENE 400", "90"),
    ])
    .unwrap();
    let again = apply(vec![row("A1", "AMOXICILLINE 500 MG", "160")]).unwrap();
    assert_eq!((again.created, again.updated, again.matched), (0, 1, 0));
    let found = shop.db.read(|c| chifa::list(c, "amox", 10)).unwrap();
    assert_eq!(
        found
            .iter()
            .map(|p| (p.name.as_str(), p.reference_price, p.reimbursable))
            .collect::<Vec<_>>(),
        [("AMOXICILLINE 500 MG", Money(16_000), true)],
        "no reimbursable column: reimbursable"
    );
    assert_eq!(shop.db.read(|c| chifa::list(c, "", 10)).unwrap().len(), 2);
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'pharmacy.chifa_applied'"),
        2
    );
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Staff]))
        .unwrap();
    assert_eq!(
        apply(vec![row("C3", "AUTRE", "10")]).unwrap_err().code(),
        "module_disabled"
    );
}
