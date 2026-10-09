//! MVP completion (Task 16).

use crate::auth::{roles, users};
use crate::catalog::products::{ProductInput, ProductKind};
use crate::catalog::variant_grid;
use crate::catalog::variants::{self, VariantAxis, VariantsInput};
use crate::labels::build::{LabelBarcode, LabelSymbology};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payment_types::{self as pt, PaymentTypeInput, TenderType};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::reports::{ReportKind, ReportRequest, run};
use crate::sales::{Cart, CartLine, SaleExtras, SaleInput, create_sale_with};
use crate::table::Cell;
use crate::testkit::shop;

const CCP_DEBIT: &str = "00000000-0000-7000-8000-000000000601";

#[test]
fn a_custom_payment_type_rides_its_base_tender_and_is_counted_apart() {
    let shop = shop();
    shop.open_till(100_000);
    let tv = shop.product("Téléviseur", Money(6_000_000), &shop.tva19).id;
    let sell = |payments: Vec<PaymentInput>, types: Vec<TenderType>| {
        let ctx = shop.till();
        let input = SaleInput {
            cart: Cart::new(Lang::Fr, vec![CartLine::product(&tv, Qty(1_000))]),
            payments,
        };
        let extras = SaleExtras {
            payment_types: types,
            ..SaleExtras::default()
        };
        shop.db
            .write(move |tx| create_sale_with(tx, &ctx, input, &extras))
    };
    let card = PaymentInput::electronic(PaymentMethod::CardCib, Money(6_000_000), Some("A1"));
    let err = sell(
        vec![card],
        vec![TenderType {
            payment: 0,
            payment_type_id: CCP_DEBIT.into(),
        }],
    )
    .unwrap_err();
    assert_eq!(err.code(), "payment_type_mismatch");
    let bare = PaymentInput::electronic(PaymentMethod::Transfer, Money(6_000_000), None);
    let err = sell(
        vec![bare],
        vec![TenderType {
            payment: 0,
            payment_type_id: CCP_DEBIT.into(),
        }],
    )
    .unwrap_err();
    assert_eq!(
        (err.field(), err.code().as_str()),
        (Some("payments.0.reference"), "transfer_reference_required"),
        "M2 already wants a transfer's reference: the base rule speaks first"
    );
    let actor = shop.owner.actor();
    let tpe = shop
        .db
        .write(move |tx| pt::create(tx, &actor, PaymentTypeInput::sample()))
        .unwrap();
    let card = PaymentInput::electronic(PaymentMethod::CardCib, Money(6_000_000), None);
    let err = sell(
        vec![card],
        vec![TenderType {
            payment: 0,
            payment_type_id: tpe.id,
        }],
    )
    .unwrap_err();
    assert_eq!(
        (err.field(), err.code().as_str()),
        (Some("payments.0.reference"), "required"),
        "a type that requires a reference refuses a card tender without one"
    );
    let debit = PaymentInput::electronic(
        PaymentMethod::Transfer,
        Money(6_000_000),
        Some("PRLV 2026-09 n° 4471"),
    );
    let sale = sell(
        vec![debit],
        vec![TenderType {
            payment: 0,
            payment_type_id: CCP_DEBIT.into(),
        }],
    )
    .unwrap();
    assert_eq!(
        sale.document.stamp_duty,
        Money::ZERO,
        "a detailed CCP debit pays no duty, like its base transfer"
    );
    assert_eq!(shop.count("SELECT COUNT(*) FROM payment_type_uses"), 1);
    let req = ReportRequest::new(ReportKind::PaymentTypes, "2026-09-28", "2026-09-28");
    let report = shop.db.read(move |c| run(c, &req)).unwrap();
    let row = report
        .table
        .rows
        .iter()
        .find(|r| r[1] == Cell::text("Prélèvement CCP"))
        .unwrap();
    assert_eq!(&row[2..4], &[Cell::int(1), Cell::money(Money(6_000_000))]);
    let actor = shop.owner.actor();
    let dup = PaymentTypeInput {
        code: "ccp_debit".into(),
        name_ar: String::new(),
        name_fr: "Autre".into(),
        name_en: String::new(),
        base_method: PaymentMethod::Transfer,
        requires_reference: true,
        sort_order: 2,
    };
    assert_eq!(
        shop.db
            .write(move |tx| pt::create(tx, &actor, dup))
            .unwrap_err()
            .code(),
        "payment_type_code_taken"
    );
    let actor = shop.owner.actor();
    let cash_type = PaymentTypeInput {
        code: "tickets_resto".into(),
        base_method: PaymentMethod::Cash,
        ..PaymentTypeInput::sample()
    };
    assert_eq!(
        shop.db
            .write(move |tx| pt::create(tx, &actor, cash_type))
            .unwrap_err()
            .code(),
        "payment_method_invalid"
    );
}

#[test]
fn a_role_or_a_user_is_duplicated_as_a_template() {
    let shop = shop();
    let actor = shop.owner.actor();
    let role = shop.cashier.role_id.clone();
    let copy = shop
        .db
        .write(move |tx| {
            roles::duplicate(
                tx,
                &actor,
                &role,
                ("بائع الصباح", "Caissier du matin", "Morning cashier"),
            )
        })
        .unwrap();
    let role = shop.cashier.role_id.clone();
    let original = shop.db.read(move |c| roles::get(c, &role)).unwrap();
    assert_eq!(
        (
            copy.permissions.clone(),
            copy.is_system,
            copy.name_fr.as_str()
        ),
        (original.permissions, false, "Caissier du matin")
    );
    let actor = shop.owner.actor();
    let template = shop.cashier.user_id.clone();
    let twin = shop
        .db
        .write(move |tx| {
            users::duplicate(tx, &actor, &template, "Sara", crate::testkit::FAKE_PIN_HASH)
        })
        .unwrap();
    assert_eq!(
        (
            twin.role_id,
            twin.lang,
            twin.discount_limit_bp,
            twin.display_name.as_str()
        ),
        (shop.cashier.role_id.clone(), Lang::Fr, 1_000, "Sara")
    );
}

#[test]
fn the_variant_grid_shows_stock_per_size_and_colour() {
    let shop = shop();
    let mut model = ProductInput::simple(
        "Chemise Oxford",
        crate::catalog::UNIT_PIECE_ID,
        &shop.tva19,
        Money(250_000),
    );
    model.kind = ProductKind::VariantParent;
    let parent = shop.product_with(model).id;
    let actor = shop.owner.actor();
    let p = parent.clone();
    let made = shop
        .db
        .write(move |tx| {
            variants::generate(
                tx,
                &actor,
                &p,
                VariantsInput {
                    axes: vec![
                        VariantAxis {
                            name: "Taille".into(),
                            values: vec!["S".into(), "M".into(), "L".into()],
                        },
                        VariantAxis {
                            name: "Couleur".into(),
                            values: vec!["Blanc".into(), "Bleu".into()],
                        },
                    ],
                    sell_price: None,
                    cost_price: None,
                    internal_barcodes: true,
                },
            )
        })
        .unwrap();
    assert_eq!(made.len(), 6);
    shop.stock_in(&made[0].id, 4_000);
    shop.stock_in(&made[5].id, 2_000);
    let p = parent.clone();
    let grid = shop
        .db
        .read(move |c| variant_grid::grid(c, &p, None, Some("Taille")))
        .unwrap();
    assert_eq!(
        (grid.row_axis.as_str(), grid.col_axis.as_deref()),
        ("Taille", Some("Couleur"))
    );
    assert_eq!(
        (grid.rows.clone(), grid.cols.clone()),
        (
            vec!["S".to_owned(), "M".into(), "L".into()],
            vec!["Blanc".to_owned(), "Bleu".into()]
        )
    );
    let at = |r: &str, c: &str| {
        grid.cells
            .iter()
            .find(|x| x.row_value == r && x.col_value.as_deref() == Some(c))
            .unwrap()
            .on_hand
    };
    assert_eq!(
        (at("S", "Blanc"), at("L", "Bleu"), at("M", "Blanc")),
        (Qty(4_000), Qty(2_000), Qty::ZERO)
    );
    let p = parent.clone();
    let default = shop
        .db
        .read(move |c| variant_grid::grid(c, &p, None, None))
        .unwrap();
    assert_eq!(
        (default.row_axis.as_str(), default.cols.clone()),
        ("Couleur", vec!["S".to_owned(), "M".into(), "L".into()]),
        "rows default to the first axis name in alphabetical order"
    );
    let p = parent.clone();
    assert_eq!(
        shop.db
            .read(move |c| variant_grid::grid(c, &p, None, Some("Saison")))
            .unwrap_err()
            .code(),
        "variant_axes_invalid"
    );
}

#[test]
fn code39_and_itf_labels_accept_only_their_characters() {
    let ok = |s: LabelSymbology, d: &str| {
        LabelBarcode {
            symbology: s,
            data: d.into(),
        }
        .check()
        .is_ok()
    };
    assert!(ok(LabelSymbology::Code39, "REF-2026 A/12"));
    assert!(
        !ok(LabelSymbology::Code39, "ref-2026"),
        "Code 39 has no lowercase"
    );
    assert!(ok(LabelSymbology::Itf, "12345678901234"));
    assert!(!ok(LabelSymbology::Itf, "123"), "ITF pairs digits");
    assert!(!ok(LabelSymbology::Itf, "12AB"));
}

#[test]
fn there_are_at_least_fifty_reports() {
    assert!(
        ReportKind::ALL.len() >= 50,
        "RPT-079: {} reports",
        ReportKind::ALL.len()
    );
}
