//! M2 command guards and redaction (Review Focus 2 and 5), through the same functions the
//! `#[tauri::command]` wrappers call.

use super::testshop::{file_printer, shop, shop_in, wait_for_bytes};
use super::{cash, credit, customers, documents, purchases, redact, stock, suppliers};
use super::{expenses as expense_cmds, product_extras, promotions as promo_cmds, stock_m2, users};
use pos_core::auth::{Permission, STOREKEEPER_ROLE_ID};
use pos_core::cash::CloseInput;
use pos_core::cash::closing::CountLine;
use pos_core::customers::credit::CollectInput;
use pos_core::customers::{CustomerInput, CustomerQuery};
use pos_core::documents::{Document, DocumentQuery};
use pos_core::expenses::NewExpense;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::payments::{PaymentInput, PaymentMethod};
use pos_core::purchases::receipts::ReceiptInput;
use pos_core::purchases::{PurchaseLineInput, PurchaseQuery};
use pos_core::sales::{Cart, CartLine, SaleInput};
use pos_core::stock::LocationInput;
use pos_core::stock::counts::{CountInput, CountMode, CountScope};
use pos_core::stock::transfers::{TransferInput, TransferLine};
use pos_core::stock::views::{LevelQuery, MovementQuery};
use pos_core::suppliers::{SupplierInput, SupplierPaymentInput};
use pos_core::tax::tva::Discount;
use pos_docs::receipt::label;
use pos_hw::fmt::amount;
use pos_hw::layout::Row;
use pos_hw::profile::{PaperWidth, Preset, Profile};
use serde_json::json;
use std::path::Path;

fn versement(customer_id: &str, amount: i64) -> CollectInput {
    CollectInput {
        customer_id: customer_id.into(),
        amount: Money(amount),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        allocate_to: Vec::new(),
        note: None,
    }
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn balances_are_kridi_data_and_customers_are_edited_by_managers() {
    let shop = shop();
    shop.owner();
    let mut limited = CustomerInput::named("Karim");
    limited.credit_limit = Some(Money(500_000));
    let karim = customers::create(&shop.st, limited).unwrap();
    credit::set_opening(&shop.st, karim.id.clone(), Money(-150_000), None).unwrap();
    let full = customers::get(&shop.st, karim.id.clone()).unwrap();
    assert_eq!(
        (full.balance, full.available_credit, full.store_credit),
        (Money(-150_000), Some(Money(500_000)), Money(150_000))
    );
    let hit = customers::search(&shop.st, CustomerQuery::default())
        .unwrap()
        .remove(0);
    let clerk = shop.user("Rachid", STOREKEEPER_ROLE_ID, "1357", 0);
    let storekeeper = shop.sign_in(&clerk, "1357");
    let mut seen = full.clone();
    redact::customer(&mut seen, &storekeeper);
    assert_eq!(
        (
            seen.balance,
            seen.credit_limit,
            seen.available_credit,
            seen.store_credit
        ),
        (Money::ZERO, None, None, Money::ZERO)
    );
    let mut seen = hit.clone();
    redact::customer_hit(&mut seen, &storekeeper);
    assert_eq!(
        (
            seen.balance,
            seen.credit_limit,
            seen.available_credit,
            seen.store_credit
        ),
        (Money::ZERO, None, None, Money::ZERO)
    );
    assert_eq!(
        customers::get(&shop.st, karim.id.clone())
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    shop.cashier();
    assert_eq!(
        customers::get(&shop.st, karim.id.clone()).unwrap().balance,
        Money(-150_000),
        "the cashier role holds credit.view"
    );
    assert_eq!(
        customers::update(&shop.st, karim.id.clone(), CustomerInput::named("K"))
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    let quick =
        customers::quick_create(&shop.st, "Nadia".into(), Some("0661 23 45 67".into())).unwrap();
    assert_eq!(quick.phone.as_deref(), Some("0661234567"));
}

/// A manager whose role lacks `credit.view` edits a customer he sees without his limit: the
/// limit he could not see is kept, not wiped.
#[test]
fn an_edit_without_credit_view_keeps_the_credit_limit() {
    let shop = shop();
    shop.owner();
    let karim = customers::create(
        &shop.st,
        CustomerInput {
            credit_limit: Some(Money(500_000)),
            ..CustomerInput::named("Karim")
        },
    )
    .unwrap();
    let role = super::users::create_role(
        &shop.st,
        pos_core::auth::RoleInput {
            name_ar: "مكلف بالزبائن".into(),
            name_fr: "Fichier clients".into(),
            name_en: "Customer desk".into(),
            permissions: vec![Permission::CustomersView, Permission::CustomersManage],
        },
    )
    .unwrap();
    let desk = shop.user("Lina", &role.id, "8642", 0);
    shop.sign_in(&desk, "8642");
    let seen = customers::get(&shop.st, karim.id.clone()).unwrap();
    assert_eq!(seen.credit_limit, None, "redacted");
    let edited = customers::update(
        &shop.st,
        karim.id.clone(),
        CustomerInput {
            name: "Karim Benali".into(),
            ..CustomerInput::from(&seen)
        },
    )
    .unwrap();
    assert_eq!(
        (edited.name.as_str(), edited.credit_limit),
        ("Karim Benali", None)
    );
    shop.owner();
    assert_eq!(
        customers::get(&shop.st, karim.id).unwrap().credit_limit,
        Some(Money(500_000))
    );
}

#[test]
fn a_versement_needs_an_open_till_and_prints_its_receipt() {
    let shop = shop();
    let owner = shop.owner();
    shop.st
        .db
        .write(move |tx| {
            pos_core::settings::set_value(
                tx,
                &owner.actor(),
                "stamp_duty.mode",
                json!("charge_customer"),
            )
        })
        .unwrap();
    let karim = customers::create(&shop.st, CustomerInput::named("Karim")).unwrap();
    credit::set_opening(&shop.st, karim.id.clone(), Money(300_000), None).unwrap();
    shop.cashier();
    let quote = credit::quote(&shop.st, versement(&karim.id, 200_000)).unwrap();
    assert_eq!(
        quote.stamp_duty,
        Money(2_000),
        "1 DA per 100 DA of cash, from the backend"
    );
    let request = |print_receipt| credit::CollectRequest {
        input: versement(&karim.id, 200_000),
        print_receipt,
    };
    assert_eq!(
        credit::collect(&shop.st, request(false))
            .unwrap_err()
            .code(),
        "cash_session_closed"
    );
    shop.open_till(0);
    let done = credit::collect(&shop.st, request(true)).unwrap();
    assert!(done.print_queued);
    assert_eq!(done.collected.customer.balance, Money(100_000));
    assert!(
        done.collected
            .entry
            .receipt_number
            .as_deref()
            .is_some_and(|n| n.starts_with("RV2026-"))
    );
}

#[test]
fn a_versement_receipt_prints_through_the_queue_and_a_reprint_is_a_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, _failures) = shop_in(dir.path());
    let out = dir.path().join("receipt.bin");
    shop.st.hw.save(file_printer(&out)).unwrap();
    let karim = customers::create(&shop.st, CustomerInput::named("Karim")).unwrap();
    credit::set_opening(&shop.st, karim.id.clone(), Money(300_000), None).unwrap();
    shop.cashier();
    shop.open_till(0);
    let done = credit::collect(
        &shop.st,
        credit::CollectRequest {
            input: versement(&karim.id, 200_000),
            print_receipt: true,
        },
    )
    .unwrap();
    let number = done.collected.entry.receipt_number.clone().unwrap();
    // the receipt, then the drawer opens for the cash (ESC p on pin 2, the default options)
    let kick = [0x1B, b'p', 0, 25, 250];
    let mut printed = wait_for_bytes(&out, 100);
    for _ in 0..200 {
        if contains(&printed, &kick) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        printed = std::fs::read(&out).unwrap();
    }
    assert!(contains(&printed, number.as_bytes()), "the RV number");
    assert!(contains(&printed, b"Karim"), "the customer");
    assert!(!contains(&printed, b"DUPLICATA"), "the original");
    assert!(contains(&printed, &kick), "the drawer opens for the cash");
    let entry_id = done.collected.entry.id.clone();
    let outcome = credit::reprint_receipt(&shop.st, entry_id.clone(), None).unwrap();
    assert_eq!(outcome.printer, "Caisse 1");
    assert!(contains(&std::fs::read(&out).unwrap(), b"DUPLICATA"));
    shop.owner();
    let history = super::audit::history(
        &shop.st,
        pos_core::audit::AuditFilter {
            entity_id: Some(karim.id.clone()),
            ..pos_core::audit::AuditFilter::default()
        },
    )
    .unwrap();
    assert_eq!(
        history.rows[0].action, "credit.receipt_reprinted",
        "the DUPLICATA is on the customer's history"
    );
    shop.cashier();
    assert_eq!(
        super::hardware::retry_print(&shop.st, entry_id)
            .unwrap_err()
            .code(),
        "print_not_pending",
        "a duplicate settles the original"
    );
    let statement = credit::print_statement(
        &shop.st,
        karim.id.clone(),
        "2000-01-01".into(),
        "2100-12-31".into(),
        Lang::Fr,
    )
    .unwrap();
    assert_eq!(statement.printer, "Caisse 1");
    assert!(contains(
        &std::fs::read(&out).unwrap(),
        karim.code.as_bytes()
    ));
}

#[test]
fn a_failed_versement_receipt_names_its_number_and_is_retried_as_a_versement() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, failures) = shop_in(dir.path());
    let karim = customers::create(&shop.st, CustomerInput::named("Karim")).unwrap();
    credit::set_opening(&shop.st, karim.id.clone(), Money(300_000), None).unwrap();
    shop.cashier();
    shop.open_till(0);
    let done = credit::collect(
        &shop.st,
        credit::CollectRequest {
            input: versement(&karim.id, 200_000),
            print_receipt: true,
        },
    )
    .unwrap();
    let failed = failures
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    assert_eq!(failed.document_id, done.collected.entry.id);
    assert_eq!(failed.number, done.collected.entry.receipt_number);
    assert_eq!(failed.code, "printer_not_configured");
    assert!(
        failed.versement,
        "the window reprints it with credit_reprint_receipt"
    );
    let out = dir.path().join("receipt.bin");
    shop.st.hw.save(file_printer(&out)).unwrap();
    super::hardware::retry_print(&shop.st, failed.document_id).unwrap();
    let number = done.collected.entry.receipt_number.unwrap();
    assert!(contains(&wait_for_bytes(&out, 100), number.as_bytes()));
}

/// The opening balance has no document number: the receipt names it, in its own language.
#[test]
fn a_versement_receipt_names_the_opening_balance_it_settles() {
    let shop = shop();
    let karim = customers::create(&shop.st, CustomerInput::named("Karim")).unwrap();
    credit::set_opening(&shop.st, karim.id.clone(), Money(300_000), None).unwrap();
    shop.cashier();
    shop.open_till(0);
    let done = credit::collect(
        &shop.st,
        credit::CollectRequest {
            input: versement(&karim.id, 200_000),
            print_receipt: false,
        },
    )
    .unwrap();
    let settled = |lang| {
        crate::printing::load_settlement(&shop.st.db, &done.collected.entry.id, false, Some(lang))
            .unwrap()
            .settled
    };
    let line = |number: &str| pos_docs::settlement::SettledLine {
        number: number.into(),
        amount: Money(200_000),
    };
    assert_eq!(settled(Lang::Fr), [line("Solde d'ouverture")]);
    assert_eq!(settled(Lang::Ar), [line("رصيد افتتاحي")]);
    assert_eq!(settled(Lang::En), [line("Opening balance")]);
}

/// The bytes `print` appended to the file printer's output.
fn printed_by(out: &Path, print: impl FnOnce()) -> Vec<u8> {
    let before = std::fs::read(out).map_or(0, |b| b.len());
    print();
    std::fs::read(out).unwrap().split_off(before)
}

fn find(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    bytes.windows(needle.len()).position(|w| w == needle)
}

/// Since Task 4 a kridi versement taken in cash counts in the expected cash: the X and the Z
/// print it on its own line, just before "Espèces attendues", so the drawer lines on paper add
/// up to it. The cashier's blind X (CSH-004) prints neither.
#[test]
fn the_x_and_the_z_print_the_cash_versements_counted_in_the_expected_cash() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, _failures) = shop_in(dir.path());
    let out = dir.path().join("receipt.bin");
    shop.st.hw.save(file_printer(&out)).unwrap();
    let karim = customers::create(&shop.st, CustomerInput::named("Karim")).unwrap();
    credit::set_opening(&shop.st, karim.id.clone(), Money(300_000), None).unwrap();
    shop.cashier();
    let session = shop.open_till(500_000);
    credit::collect(
        &shop.st,
        credit::CollectRequest {
            input: versement(&karim.id, 200_000),
            print_receipt: false,
        },
    )
    .unwrap();
    let page = Profile::preset(Preset::Generic, PaperWidth::Mm80).code_page;
    let versements = page.encode(&label(Lang::Fr, "z.creditPayments"));
    let expected = page.encode(&label(Lang::Fr, "z.expected"));
    // 2 000 DA in cash + 20 DA of duty charged to the customer (the default stamp mode)
    let figure = page.encode(&amount(Money(202_000), Lang::Fr));
    let check = |bytes: &[u8]| {
        let at = find(bytes, &versements).expect("the versements line");
        let eol = at + bytes[at..].iter().position(|b| *b == b'\n').unwrap();
        assert!(contains(&bytes[at..eol], &figure), "2 020,00 on its line");
        let next = find(bytes, &expected).expect("the expected cash line");
        assert!(
            next > eol && !bytes[eol + 1..next].contains(&b'\n'),
            "the line just before the expected cash"
        );
    };

    let blind = printed_by(&out, || {
        cash::print_x(&shop.st, Lang::Fr).unwrap();
    });
    assert!(contains(&blind, b"Rapport X"));
    assert!(!contains(&blind, &versements), "a blind X adds nothing up");
    assert!(!contains(&blind, &expected));

    shop.owner();
    let x = cash::x_report(&shop.st).unwrap();
    assert_eq!(
        (x.credit_payments, x.expected_cash),
        (Money(202_000), Some(Money(702_000))),
        "5 000 DA of float + 2 020 DA of versement"
    );
    check(&printed_by(&out, || {
        cash::print_x(&shop.st, Lang::Fr).unwrap();
    }));
    // a summary whose expected cash is withheld still prints the versements with the other
    // drawer lines, right after the expenses
    let mut withheld = x.clone();
    withheld.expected_cash = None;
    let rows = pos_hw::report::x_layout(&withheld, "Shop", Lang::Fr, 0).rows;
    let line = |key: &str| {
        let wanted = label(Lang::Fr, key);
        rows.iter()
            .position(|r| matches!(r, Row::Pair { start, .. } if *start == wanted))
    };
    assert_eq!(line("z.creditPayments"), line("z.expenses").map(|i| i + 1));
    let ar = pos_hw::report::x_layout(&x, "Shop", Lang::Ar, 0);
    assert!(ar.rtl && ar.texts().any(|t| t == "دفعات الكريدي نقداً"));

    shop.cashier();
    let count = CloseInput {
        counts: vec![
            CountLine {
                denomination: Money(200_000),
                count: 3,
            },
            CountLine {
                denomination: Money(100_000),
                count: 1,
            },
            CountLine {
                denomination: Money(2_000),
                count: 1,
            },
        ],
        notes: None,
    };
    let closed = cash::close_session(&shop.st, count).unwrap();
    assert_eq!(
        (closed.z.summary.credit_payments, closed.z.variance),
        (Money(202_000), Money::ZERO)
    );
    let z = printed_by(&out, || {
        cash::print_z(&shop.st, session.id, Lang::Fr).unwrap();
    });
    assert!(contains(&z, b"Rapport Z"));
    check(&z);
}

#[test]
fn a_ticket_on_credit_carries_its_customer_and_the_balance_after_it() {
    let shop = shop();
    let karim = customers::create(&shop.st, CustomerInput::named("Karim")).unwrap();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, None);
    shop.stock_in(&milk.id, 10);
    shop.cashier();
    shop.open_till(0);
    let sell = |tender: PaymentInput| {
        let ctx = shop.st.till(Permission::SalesCreate).unwrap();
        let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&milk.id, Qty(2_000))]);
        cart.customer_id = Some(karim.id.clone());
        let input = SaleInput {
            cart,
            payments: vec![tender],
        };
        shop.st
            .db
            .write(move |tx| pos_core::sales::create_sale(tx, &ctx, input))
            .unwrap()
    };
    let on_credit = sell(PaymentInput {
        method: PaymentMethod::Credit,
        ..PaymentInput::cash(Money(30_000))
    });
    let (receipt, _) =
        crate::printing::load_receipt(&shop.st.db, &on_credit.document.id, false, None).unwrap();
    let customer = receipt.customer.unwrap();
    assert_eq!((customer.name, customer.code), ("Karim".into(), karim.code));
    assert_eq!(receipt.customer_balance, Some(Money(30_000)));
    let in_cash = sell(PaymentInput::cash(Money(30_000)));
    let (receipt, _) =
        crate::printing::load_receipt(&shop.st.db, &in_cash.document.id, false, None).unwrap();
    assert!(receipt.customer.is_some());
    assert_eq!(
        receipt.customer_balance, None,
        "the ticket did not move the account"
    );
}

#[test]
fn paying_a_supplier_from_the_drawer_needs_cash_paid_in_out() {
    let shop = shop();
    shop.owner();
    let cevital = suppliers::create(&shop.st, SupplierInput::named("Cevital")).unwrap();
    suppliers::set_opening(&shop.st, cevital.id.clone(), Money(500_000), None).unwrap();
    shop.open_till(1_000_000);
    let payment = |from_till| SupplierPaymentInput {
        supplier_id: cevital.id.clone(),
        amount: Money(200_000),
        method: PaymentMethod::Cash,
        from_till,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        allocate_to: Vec::new(),
        note: None,
    };
    let clerk = shop.user("Rachid", STOREKEEPER_ROLE_ID, "1357", 0);
    shop.sign_in(&clerk, "1357");
    assert_eq!(
        suppliers::pay(&shop.st, payment(true)).unwrap_err().code(),
        "insufficient_permission"
    );
    assert_eq!(
        suppliers::pay(&shop.st, payment(false)).unwrap().amount,
        Money(-200_000),
        "a payment from the safe is the storekeeper's job"
    );
    shop.owner();
    assert_eq!(
        suppliers::pay(&shop.st, payment(true)).unwrap().amount,
        Money(-200_000)
    );
    assert!(
        purchases::list(&shop.st, PurchaseQuery::default())
            .unwrap()
            .is_empty()
    );
}

/// The storekeeper receives the goods and types their cost, but the CMUP it moves is a cost
/// (`reports.profit`), like everywhere else.
#[test]
fn the_average_cost_a_receipt_moves_is_shown_with_reports_profit_only() {
    let shop = shop();
    let cevital = suppliers::create(&shop.st, SupplierInput::named("Cevital")).unwrap();
    let oil = shop.product("Huile Elio 1 L", 25_000, 900, None);
    let clerk = shop.user("Rachid", STOREKEEPER_ROLE_ID, "1357", 0);
    shop.sign_in(&clerk, "1357");
    let done = purchases::receive(
        &shop.st,
        ReceiptInput {
            supplier_id: cevital.id.clone(),
            order_id: None,
            location_id: None,
            lines: vec![PurchaseLineInput {
                product_id: oil.id.clone(),
                qty: Qty(12_000),
                unit_cost: Money(18_000),
                pack_qty: None,
                tax_rate_id: None,
                discount: Discount::None,
                origin_line_id: None,
                lot_number: None,
                expiry_date: None,
                serials: Vec::new(),
                note: None,
            }],
            supplier_ref: Some("BL-778".into()),
            note: None,
            invoice: None,
        },
    )
    .unwrap();
    assert!(done.receipt.number.is_some());
    assert!(
        done.cost_changes
            .iter()
            .all(|c| (c.cost_before, c.cost_after) == (Money::ZERO, Money::ZERO))
    );
    let history = purchases::cost_history(&shop.st, oil.id.clone()).unwrap();
    assert!(!history.is_empty());
    assert!(history.iter().all(|c| c.cost_after == Money::ZERO));
    shop.owner();
    let history = purchases::cost_history(&shop.st, oil.id).unwrap();
    assert!(history.iter().any(|c| c.cost_after != Money::ZERO));
}

#[test]
fn a_product_photo_is_resized_into_media_and_nowhere_else() {
    let shop = shop();
    shop.owner();
    let oil = shop.product("Huile 5 L", 119_000, 1_900, Some("6130000000015"));
    let dir = tempfile::tempdir().unwrap();
    let mut png = Vec::new();
    image::DynamicImage::new_rgb8(1600, 1200)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let saved =
        product_extras::set_image_bytes(&shop.st, dir.path(), oil.id.clone(), &png).unwrap();
    let name = saved.image.clone().unwrap();
    assert!(name.starts_with("products/") && name.ends_with(".jpg"));
    let full = image::open(dir.path().join(&name)).unwrap();
    assert!(full.width() <= 512 && full.height() <= 512);
    assert!(
        dir.path()
            .join(pos_core::catalog::images::thumb_name(&name).unwrap())
            .exists()
    );
    assert_eq!(
        product_extras::set_image_bytes(&shop.st, dir.path(), oil.id.clone(), b"not an image")
            .unwrap_err()
            .code(),
        "image_invalid"
    );
    // A new photo replaces the old one: its files go once the product points at the new one.
    let again =
        product_extras::set_image_bytes(&shop.st, dir.path(), oil.id.clone(), &png).unwrap();
    let second = again.image.clone().unwrap();
    assert_ne!(second, name);
    assert!(!dir.path().join(&name).exists());
    assert!(dir.path().join(&second).exists());
    let cleared = product_extras::clear_image(&shop.st, dir.path(), oil.id.clone()).unwrap();
    assert_eq!(cleared.image, None);
    assert!(!dir.path().join(&second).exists());
    let left = std::fs::read_dir(dir.path().join("products"))
        .unwrap()
        .count();
    assert_eq!(left, 0, "no file left behind");
    shop.cashier();
    assert_eq!(
        product_extras::set_image_bytes(&shop.st, dir.path(), oil.id, &png)
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
}

#[test]
fn promotions_need_products_price_and_presets_reach_the_till() {
    let shop = shop();
    shop.cashier();
    let input = pos_core::promotions::PromotionInput {
        name_ar: String::new(),
        name_fr: "Remise fidèle".into(),
        name_en: String::new(),
        kind: pos_core::promotions::PromoKind::Percent,
        campaign: pos_core::promotions::Campaign::Promotion,
        trigger_mode: pos_core::promotions::TriggerMode::Manual,
        scope: pos_core::promotions::PromoScope::All,
        value: 500,
        buy_qty: None,
        get_qty: None,
        min_qty: Qty::ZERO,
        priority: 0,
        starts_on: None,
        ends_on: None,
        active: true,
    };
    assert_eq!(
        promo_cmds::create(&shop.st, input.clone())
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    shop.manager();
    promo_cmds::create(&shop.st, input).unwrap();
    shop.cashier();
    assert_eq!(
        promo_cmds::presets(&shop.st).unwrap().len(),
        1,
        "named discounts for the till (PRM-003)"
    );
}

#[test]
fn counts_and_expenses_follow_their_permissions() {
    let shop = shop();
    shop.owner();
    let oil = shop.product("Huile 5 L", 119_000, 1_900, None);
    shop.stock_in(&oil.id, 10);
    shop.cashier();
    let full = CountInput {
        location_id: None,
        scope: CountScope::Full,
        category_id: None,
        product_ids: Vec::new(),
        blind: true,
        note: None,
    };
    assert_eq!(
        stock_m2::start_count(&shop.st, full.clone())
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    let clerk = shop.user("Rachid", STOREKEEPER_ROLE_ID, "1357", 0);
    shop.sign_in(&clerk, "1357");
    let count = stock_m2::start_count(&shop.st, full).unwrap();
    let line = stock_m2::set_counted(
        &shop.st,
        count.id.clone(),
        oil.id.clone(),
        Qty(9_000),
        CountMode::Set,
    )
    .unwrap();
    assert_eq!(line.counted_qty, Some(Qty(9_000)));
    shop.cashier();
    shop.open_till(50_000);
    let taxi = NewExpense {
        category_id: "00000000-0000-7000-8000-000000000505".into(),
        amount: Money(80_000),
        method: PaymentMethod::Cash,
        from_till: true,
        reference: None,
        supplier_id: None,
        business_date: None,
        note: None,
    };
    let recorded = expense_cmds::record(&shop.st, taxi).unwrap();
    assert!(recorded.from_till);
    assert_eq!(
        expense_cmds::list(&shop.st, Default::default())
            .unwrap_err()
            .code(),
        "insufficient_permission",
        "the cashier records, the manager reads"
    );
    shop.manager();
    assert_eq!(
        expense_cmds::list(&shop.st, Default::default())
            .unwrap()
            .len(),
        1
    );
}

/// STK-011: a blind count shows the counter neither the expected quantity nor the variance
/// unless they may adjust the stock anyway; a scan adds one pack of the code read.
#[test]
fn a_blind_count_hides_what_is_expected_from_a_counter_without_stock_adjust() {
    let shop = shop();
    shop.owner();
    let oil = shop.product("Huile 5 L", 119_000, 1_900, Some("6130000000015"));
    shop.stock_in(&oil.id, 10);
    let role = users::create_role(
        &shop.st,
        pos_core::auth::RoleInput {
            name_ar: String::new(),
            name_fr: "Compteur".into(),
            name_en: String::new(),
            permissions: vec![Permission::StockCount],
        },
    )
    .unwrap();
    let counter = shop.user("Walid", &role.id, "8642", 0);
    shop.sign_in(&counter, "8642");
    let blind = CountInput {
        location_id: None,
        scope: CountScope::Products,
        category_id: None,
        product_ids: vec![oil.id.clone()],
        blind: true,
        note: None,
    };
    let count = stock_m2::start_count(&shop.st, blind).unwrap();
    let scanned = stock_m2::scan_count(&shop.st, count.id.clone(), "6130000000015".into()).unwrap();
    assert_eq!(scanned.counted_qty, Some(Qty(1_000)));
    assert_eq!((scanned.expected_qty, scanned.variance), (None, None));
    assert_eq!(
        stock_m2::scan_count(&shop.st, count.id.clone(), "0000000000000".into())
            .unwrap_err()
            .code(),
        "product_not_found"
    );
    let lines = stock_m2::count_lines(&shop.st, count.id.clone(), false).unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!((lines[0].expected_qty, lines[0].unit_cost), (None, None));
    assert!(
        stock_m2::count_lines(&shop.st, count.id.clone(), true)
            .unwrap()
            .is_empty(),
        "nothing left to count"
    );
    shop.owner();
    let seen = stock_m2::count_lines(&shop.st, count.id.clone(), false).unwrap();
    assert_eq!(seen[0].expected_qty, Some(Qty(10_000)));
    assert_eq!(seen[0].variance, Some(Qty(-9_000)));
    assert!(seen[0].unit_cost.is_some());
    shop.sign_in(&counter, "8642");
    let closed = stock_m2::close_count(
        &shop.st,
        count.id.clone(),
        pos_core::stock::counts::Uncounted::Ignore,
    )
    .unwrap();
    assert_eq!(closed.adjusted, 1);
    assert_eq!(
        closed.variance_value,
        Money::ZERO,
        "the value of the variance is a cost (reports.profit)"
    );
}

#[test]
fn stock_views_show_costs_with_reports_profit_only() {
    let shop = shop();
    shop.owner();
    let oil = shop.product("Huile 5 L", 119_000, 1_900, None);
    shop.stock_in(&oil.id, 10);
    let levels = stock_m2::levels_view(&shop.st, LevelQuery::default()).unwrap();
    let row = levels.iter().find(|r| r.product_id == oil.id).unwrap();
    assert!(row.cost_price.is_positive() && row.cost_value.is_positive());
    let clerk = shop.user("Rachid", STOREKEEPER_ROLE_ID, "1357", 0);
    shop.sign_in(&clerk, "1357");
    let levels = stock_m2::levels_view(&shop.st, LevelQuery::default()).unwrap();
    let row = levels.iter().find(|r| r.product_id == oil.id).unwrap();
    assert_eq!((row.cost_price, row.cost_value), (Money::ZERO, Money::ZERO));
    assert_eq!(row.retail_value, Money(1_190_000), "sale prices stay");
    let log = stock_m2::movement_log(&shop.st, MovementQuery::default()).unwrap();
    assert!(!log.is_empty());
    assert!(log.iter().all(|m| m.unit_cost.is_none()));
    shop.cashier();
    let damaged = pos_core::stock::AdjustBatchInput {
        location_id: None,
        reason: pos_core::stock::AdjustReason::Damage,
        lines: vec![pos_core::stock::AdjustBatchLine {
            product_id: oil.id.clone(),
            qty_delta: Qty(-1_000),
            unit_cost: None,
        }],
        note: None,
    };
    assert_eq!(
        stock_m2::adjust_batch(&shop.st, damaged)
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
}

/// Every price and total of a transfer note is a cost (the CMUP), with none of it on the lines'
/// `cost_price` alone.
fn valued_at_cost_hidden(d: &Document) -> bool {
    let zero = Money::ZERO;
    [d.subtotal_ht, d.tax_total, d.total_ttc, d.total_due] == [zero; 4]
        && d.taxes.is_empty()
        && d.lines.iter().all(|l| {
            [
                l.unit_price,
                l.total_ht,
                l.tax_amount,
                l.total_ttc,
                l.cost_price,
            ] == [zero; 5]
        })
}

/// A transfer note (BT) is valued at the CMUP: without reports.profit its prices and value are
/// hidden by every command that reads it, the till's documents and reprints included.
#[test]
fn a_transfer_note_is_valued_at_cost_for_reports_profit_only_wherever_it_is_read() {
    let shop = shop();
    shop.owner();
    let oil = shop.product("Huile 5 L", 119_000, 1_900, None);
    shop.stock_in(&oil.id, 10);
    let shelf = stock::locations(&shop.st)
        .unwrap()
        .into_iter()
        .find(|l| l.is_default)
        .unwrap()
        .id;
    let depot = LocationInput {
        code: "DEPOT".into(),
        name_fr: "Dépôt".into(),
        ..LocationInput::default()
    };
    let depot = stock::create_location(&shop.st, depot).unwrap().id;
    let two = |from: &str, to: &str| TransferInput {
        from_location_id: from.into(),
        to_location_id: to.into(),
        lines: vec![TransferLine {
            product_id: oil.id.clone(),
            qty: Qty(2_000),
            lot_number: None,
            serials: Vec::new(),
        }],
        driver_name: None,
        vehicle: None,
        note: None,
    };
    let done = stock_m2::transfer(&shop.st, two(&shelf, &depot)).unwrap();
    let at_cost = Money(2 * 71_400);
    assert_eq!(done.document.total_due, at_cost, "the owner sees the value");
    assert_eq!(done.document.lines[0].unit_price, Money(71_400));
    let (id, number) = (
        done.document.id.clone(),
        done.document.number.clone().unwrap(),
    );
    let listed = documents::list(&shop.st, DocumentQuery::default()).unwrap();
    assert_eq!(listed[0].total_due, at_cost);
    assert_eq!(
        stock_m2::transfers_list(&shop.st, 10).unwrap()[0].value,
        at_cost
    );

    shop.cashier();
    let seen = documents::get(&shop.st, id.clone()).unwrap();
    assert!(valued_at_cost_hidden(&seen), "{seen:?}");
    assert_eq!(seen.lines[0].qty, Qty(2_000), "the quantities stay");
    let found = documents::find(&shop.st, number).unwrap().unwrap();
    assert!(valued_at_cost_hidden(&found));
    shop.open_till(50_000);
    let ticket = shop.sell(&oil.id, 1, 200_000).document;
    let listed = documents::list(&shop.st, DocumentQuery::default()).unwrap();
    let row = |id: &str| listed.iter().find(|s| s.id == id).unwrap().total_due;
    assert_eq!(row(&id), Money::ZERO, "the BT's value is a cost");
    assert_eq!(row(&ticket.id), ticket.total_due, "a ticket's total is not");
    assert_eq!(
        stock_m2::transfers_list(&shop.st, 10).unwrap()[0].value,
        Money::ZERO
    );
    assert_eq!(
        documents::reprint(&shop.st, id.clone(), None)
            .unwrap_err()
            .code(),
        "insufficient_permission",
        "a BT printed again would show its prices at cost"
    );

    let clerk = shop.user("Rachid", STOREKEEPER_ROLE_ID, "1357", 0);
    shop.sign_in(&clerk, "1357");
    let back = stock_m2::transfer(&shop.st, two(&depot, &shelf)).unwrap();
    assert!(valued_at_cost_hidden(&back.document));
    assert_eq!(back.document.lines[0].qty, Qty(2_000));
    let transfers = stock_m2::transfers_list(&shop.st, 10).unwrap();
    assert_eq!(transfers.len(), 2);
    assert!(transfers.iter().all(|t| t.value == Money::ZERO));
    assert_eq!(
        documents::get(&shop.st, id).unwrap_err().code(),
        "insufficient_permission",
        "the storekeeper has no sales history"
    );
}

#[test]
fn serials_of_stock_on_hand_are_registered_by_the_storekeeper_and_found_at_the_till() {
    let shop = shop();
    shop.owner();
    let phone = shop.product("Redmi Note 13", 4_500_000, 1_900, None);
    let extras = pos_core::catalog::extras::ProductExtrasInput {
        track_serials: true,
        ..Default::default()
    };
    product_extras::set_extras(&shop.st, phone.id.clone(), extras).unwrap();
    shop.stock_in(&phone.id, 1);
    let imei = || vec!["356789101112131".to_owned()];
    shop.cashier();
    assert_eq!(
        stock_m2::register_serials(&shop.st, phone.id.clone(), imei(), None)
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    let clerk = shop.user("Rachid", STOREKEEPER_ROLE_ID, "1357", 0);
    shop.sign_in(&clerk, "1357");
    let made = stock_m2::register_serials(&shop.st, phone.id.clone(), imei(), None).unwrap();
    assert_eq!(made.len(), 1);
    shop.cashier();
    let found = stock_m2::find_serials(&shop.st, "356789101112131".into()).unwrap();
    assert_eq!(found[0].product_id, phone.id);
}

/// IMX-018: a folder of photos named by barcode or reference; the others are reported.
#[test]
fn a_folder_of_photos_named_by_barcode_is_imported_and_the_rest_reported() {
    let shop = shop();
    shop.owner();
    let oil = shop.product("Huile 5 L", 119_000, 1_900, Some("6130000000015"));
    let milk = shop.product("Lait UHT 1 L", 9_000, 900, Some("6130000000022"));
    let (folder, media) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let mut png = Vec::new();
    image::DynamicImage::new_rgb8(300, 200)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    std::fs::write(folder.path().join("6130000000015.PNG"), &png).unwrap();
    std::fs::write(folder.path().join("6130000000022.jpg"), b"not a photo").unwrap();
    std::fs::write(folder.path().join("9999999999999.jpg"), &png).unwrap();
    std::fs::write(folder.path().join("6130000000015.txt"), b"ignored").unwrap();
    let done = product_extras::import_images_from(&shop.st, media.path(), folder.path()).unwrap();
    assert_eq!(done.matched, 1);
    assert_eq!(done.unmatched, ["9999999999999.jpg"]);
    assert_eq!(done.refused, ["6130000000022.jpg"]);
    let oil = super::products::get(&shop.st, oil.id).unwrap();
    assert!(media.path().join(oil.image.unwrap()).is_file());
    assert_eq!(super::products::get(&shop.st, milk.id).unwrap().image, None);
    shop.cashier();
    assert_eq!(
        product_extras::import_images_from(&shop.st, media.path(), folder.path())
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
}

use super::{data_files, exports, imports, reports as report_cmds};
use pos_core::imports::engine::{ImportOptions, JobStatus};
use pos_core::imports::spec::{ImportEntity, ImportPolicy};
use pos_core::reports::{COST_COLUMNS, ReportGroup, ReportKind, ReportRequest};
use pos_docs::sheet::SheetFormat;

fn staged_products(shop: &super::testshop::TestShop) -> String {
    let csv =
        "Référence;Désignation;Prix de vente\r\nA1;Huile 5 L;1190,00\r\nA2;Sucre 1 kg;130,00\r\n";
    imports::stage_bytes(&shop.st, csv.as_bytes(), "articles.csv")
        .unwrap()
        .token
}

#[test]
fn apply_refuses_a_mapping_that_was_not_checked() {
    let shop = shop();
    shop.manager();
    let token = staged_products(&shop);
    let preview =
        imports::preview(&shop.st, token.clone(), 0, ImportEntity::Products, None).unwrap();
    assert_eq!(
        preview.suggested,
        vec![
            Some("reference".into()),
            Some("nameFr".into()),
            Some("sellPrice".into())
        ]
    );
    let options = ImportOptions {
        entity: ImportEntity::Products,
        policy: ImportPolicy::CreateOnly,
        file_name: "articles.csv".into(),
        header_row: preview.header_row,
        mapping: preview.suggested.clone(),
        location_id: None,
        count_id: None,
    };
    assert_eq!(
        imports::apply(&shop.st, token.clone(), 0, options.clone(), &mut |_| {})
            .unwrap_err()
            .code(),
        "import_mapping_invalid",
        "no dry run yet"
    );
    let dry = imports::dry_run(&shop.st, token.clone(), 0, options.clone()).unwrap();
    assert_eq!((dry.create, dry.fail), (2, 0));
    let changed = ImportOptions {
        policy: ImportPolicy::Upsert,
        ..options.clone()
    };
    assert_eq!(
        imports::apply(&shop.st, token.clone(), 0, changed, &mut |_| {})
            .unwrap_err()
            .code(),
        "import_mapping_invalid",
        "not the checked options"
    );
    let mut progress = Vec::new();
    let job = imports::apply(&shop.st, token.clone(), 0, options, &mut |p| {
        progress.push(p.done)
    })
    .unwrap();
    assert_eq!((job.created, progress), (2, vec![2]));
    assert_eq!(
        imports::preview(&shop.st, token, 0, ImportEntity::Products, None)
            .unwrap_err()
            .code(),
        "import_not_found",
        "the staged file is gone"
    );
    shop.cashier();
    assert_eq!(
        imports::stage_bytes(&shop.st, b"a;b\r\n1;2\r\n", "x.csv")
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
}

/// A dry run checks one apply: the same file applied again while the first apply runs (a double
/// click, a retry) is refused, so nothing is posted twice and the real import stays undoable.
#[test]
fn a_dry_run_checks_a_single_apply() {
    let shop = shop();
    shop.manager();
    let token = staged_products(&shop);
    let preview =
        imports::preview(&shop.st, token.clone(), 0, ImportEntity::Products, None).unwrap();
    let options = ImportOptions {
        entity: ImportEntity::Products,
        policy: ImportPolicy::CreateOnly,
        file_name: "articles.csv".into(),
        header_row: preview.header_row,
        mapping: preview.suggested,
        location_id: None,
        count_id: None,
    };
    imports::dry_run(&shop.st, token.clone(), 0, options.clone()).unwrap();
    let mut again = None;
    let job = imports::apply(&shop.st, token.clone(), 0, options.clone(), &mut |_| {
        again = Some(imports::apply(
            &shop.st,
            token.clone(),
            0,
            options.clone(),
            &mut |_| {},
        ));
    })
    .unwrap();
    assert_eq!(
        again
            .expect("the batch reported its progress")
            .unwrap_err()
            .code(),
        "import_mapping_invalid",
        "the check was spent by the first apply"
    );
    let jobs = imports::jobs(&shop.st).unwrap();
    assert_eq!(jobs.len(), 1, "one job only");
    assert_eq!((jobs[0].id.as_str(), jobs[0].created), (job.id.as_str(), 2));
    assert!(jobs[0].undoable, "the real import is still the latest");
}

#[test]
fn exporting_customers_needs_customers_manage_and_is_journaled() {
    let shop = shop();
    shop.owner();
    customers::create(&shop.st, CustomerInput::named("Karim")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let request = exports::ExportRequest {
        target: exports::ExportTarget::Customers {
            query: Default::default(),
        },
        format: SheetFormat::Csv,
        lang: pos_core::lang::Lang::Fr,
    };
    shop.cashier();
    assert_eq!(
        exports::export_to(&shop.st, request.clone(), &dir.path().join("c.csv"))
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    shop.manager();
    let done = exports::export_to(&shop.st, request, &dir.path().join("c.csv")).unwrap();
    assert_eq!(done.rows, 1);
    let journaled: i64 = shop
        .st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'data.exported'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(journaled, 1);
}

/// Every kind the cashier may not run is refused; the kridi reports the cashier's role may read
/// (`credit.view`: the customers group) carry no cost. The storekeeper's stock reports and
/// product file leave the costs out.
#[test]
fn a_cashier_sees_no_cost_in_any_report_or_export() {
    let shop = shop();
    shop.owner();
    shop.product("Huile 5 L", 119_000, 1_900, None);
    shop.cashier();
    let no_cost = |view: &report_cmds::ReportView| {
        view.report
            .table
            .columns
            .iter()
            .all(|c| !COST_COLUMNS.contains(&c.key.as_str()))
    };
    for kind in ReportKind::ALL {
        let run = report_cmds::run(
            &shop.st,
            ReportRequest::new(kind, "2026-09-01", "2026-09-30"),
        );
        if matches!(
            kind.permission(),
            Permission::CreditView | Permission::CustomersView
        ) {
            match kind.module() {
                // A pack's kridi report (overdue instalments) or customer register (the jeweller
                // register) is closed while its module is off.
                Some(_) => assert_eq!(
                    run.unwrap_err().code(),
                    "module_disabled",
                    "{}",
                    kind.as_str()
                ),
                None => assert!(no_cost(&run.unwrap()), "{}", kind.as_str()),
            }
        } else {
            assert_eq!(
                run.unwrap_err().code(),
                "insufficient_permission",
                "{}",
                kind.as_str()
            );
        }
    }
    let clerk = shop.user("Rachid", pos_core::auth::STOREKEEPER_ROLE_ID, "1357", 0);
    shop.sign_in(&clerk, "1357");
    for kind in ReportKind::ALL
        .into_iter()
        .filter(|k| k.group() == ReportGroup::Stock && k.required_filter().is_none())
    {
        let view = report_cmds::run(
            &shop.st,
            ReportRequest::new(kind, "2026-09-01", "2026-09-30"),
        )
        .unwrap();
        assert!(no_cost(&view), "{}", kind.as_str());
    }
    let dir = tempfile::tempdir().unwrap();
    let request = exports::ExportRequest {
        target: exports::ExportTarget::Products,
        format: SheetFormat::Csv,
        lang: pos_core::lang::Lang::Fr,
    };
    exports::export_to(&shop.st, request, &dir.path().join("p.csv")).unwrap();
    let text = std::fs::read_to_string(dir.path().join("p.csv")).unwrap();
    assert!(text.contains("Huile 5 L"), "{text}");
    assert!(!text.contains("Prix de revient"), "{text}");
}

/// Balances and limits are kridi data (`credit.view`): a customer file exported by someone who
/// manages customers without it leaves them out.
#[test]
fn a_customer_file_leaves_the_balances_out_without_credit_view() {
    let shop = shop();
    shop.owner();
    let karim = customers::create(&shop.st, CustomerInput::named("Karim")).unwrap();
    credit::set_opening(&shop.st, karim.id, Money(150_000), None).unwrap();
    let role = users::create_role(
        &shop.st,
        pos_core::auth::RoleInput {
            name_ar: "مكلف بالزبائن".into(),
            name_fr: "Fichier clients".into(),
            name_en: "Customer desk".into(),
            permissions: vec![Permission::CustomersView, Permission::CustomersManage],
        },
    )
    .unwrap();
    let desk = shop.user("Lina", &role.id, "8642", 0);
    shop.sign_in(&desk, "8642");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clients.csv");
    let request = exports::ExportRequest {
        target: exports::ExportTarget::Customers {
            query: Default::default(),
        },
        format: SheetFormat::Csv,
        lang: pos_core::lang::Lang::Fr,
    };
    exports::export_to(&shop.st, request.clone(), &path).unwrap();
    let seen = std::fs::read_to_string(&path).unwrap();
    let balance = pos_docs::sheet::column_label(Lang::Fr, "balance").unwrap();
    let limit = pos_docs::sheet::column_label(Lang::Fr, "creditLimit").unwrap();
    assert!(seen.contains("Karim"), "{seen}");
    assert!(!seen.contains(&balance) && !seen.contains(&limit), "{seen}");
    assert!(!seen.contains("1500,00"), "{seen}");
    shop.owner();
    exports::export_to(&shop.st, request, &path).unwrap();
    let full = std::fs::read_to_string(&path).unwrap();
    assert!(
        full.contains(&balance) && full.contains("1500,00"),
        "{full}"
    );
}

/// A staged file is its user's alone and leaves with them; the latest import of the day is
/// undone by whoever may import that kind; the history shows only the kinds the user imports.
#[test]
fn staged_files_belong_to_their_user_and_the_latest_import_can_be_undone() {
    let shop = shop();
    shop.manager();
    let token = staged_products(&shop);
    let clerk = shop.user("Rachid", pos_core::auth::STOREKEEPER_ROLE_ID, "1357", 0);
    shop.sign_in(&clerk, "1357");
    assert_eq!(
        imports::preview(&shop.st, token.clone(), 0, ImportEntity::Products, None)
            .unwrap_err()
            .code(),
        "import_not_found",
        "another user's file"
    );
    shop.manager();
    assert_eq!(
        imports::preview(&shop.st, token.clone(), 0, ImportEntity::Products, None)
            .unwrap_err()
            .code(),
        "import_not_found",
        "dropped when its user signed out"
    );
    let token = staged_products(&shop);
    imports::discard(&shop.st, token.clone()).unwrap();
    assert_eq!(
        imports::preview(&shop.st, token, 0, ImportEntity::Products, None)
            .unwrap_err()
            .code(),
        "import_not_found"
    );
    let token = staged_products(&shop);
    assert_eq!(
        imports::preview(&shop.st, token.clone(), 3, ImportEntity::Products, None)
            .unwrap_err()
            .code(),
        "import_sheet_not_found"
    );
    let preview =
        imports::preview(&shop.st, token.clone(), 0, ImportEntity::Products, None).unwrap();
    assert_eq!(
        preview.headers,
        ["Référence", "Désignation", "Prix de vente"]
    );
    assert_eq!(preview.sample.len(), 2);
    let price = preview
        .fields
        .iter()
        .find(|f| f.key == "sellPrice")
        .unwrap();
    assert_eq!(price.label, "Prix de vente");
    let options = ImportOptions {
        entity: ImportEntity::Products,
        policy: ImportPolicy::CreateOnly,
        file_name: "autre-nom.csv".into(),
        header_row: preview.header_row,
        mapping: preview.suggested,
        location_id: None,
        count_id: None,
    };
    imports::dry_run(&shop.st, token.clone(), 0, options.clone()).unwrap();
    let job = imports::apply(&shop.st, token, 0, options, &mut |_| {}).unwrap();
    assert_eq!(job.file_name, "articles.csv", "the name of the file read");
    assert!(job.undoable);

    shop.owner();
    let role = users::create_role(
        &shop.st,
        pos_core::auth::RoleInput {
            name_ar: String::new(),
            name_fr: "Fichier clients".into(),
            name_en: String::new(),
            permissions: vec![Permission::CustomersManage],
        },
    )
    .unwrap();
    let desk = shop.user("Lina", &role.id, "8642", 0);
    shop.sign_in(&desk, "8642");
    assert!(
        imports::jobs(&shop.st).unwrap().is_empty(),
        "a product import is not the customer desk's"
    );
    assert_eq!(
        imports::undo(&shop.st, job.id.clone()).unwrap_err().code(),
        "insufficient_permission"
    );
    shop.cashier();
    assert_eq!(
        imports::jobs(&shop.st).unwrap_err().code(),
        "insufficient_permission"
    );
    shop.sign_in(&clerk, "1357");
    assert_eq!(imports::jobs(&shop.st).unwrap().len(), 1);
    let undone = imports::undo(&shop.st, job.id).unwrap();
    assert_eq!(undone.status, JobStatus::Undone);
}

/// The `.dzpos` file of a fresh backup goes out and comes back into the backups folder, and the
/// whole data leaves as CSV files: `settings.data`, each journaled.
#[test]
fn backup_files_and_the_full_export_need_settings_data_and_are_journaled() {
    let dir = tempfile::tempdir().unwrap();
    let paths = crate::state::AppPaths::new(dir.path());
    let st = std::sync::Arc::new(
        crate::state::AppState::open(&paths, crate::printing::silent()).unwrap(),
    );
    let owner = super::tests::first_run(&st);
    let shop = super::testshop::TestShop { st, owner };
    let out = tempfile::tempdir().unwrap();
    let file = out.path().join("boutique.dzpos");
    shop.manager();
    assert_eq!(
        data_files::export_backup_to(&shop.st, &file)
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    assert_eq!(
        data_files::export_all_to(&shop.st, &out.path().join("donnees.zip"))
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    shop.owner();
    data_files::export_backup_to(&shop.st, &file).unwrap();
    assert!(file.is_file());
    let before = shop.st.db.list_backups().unwrap().len();
    let placed = data_files::import_backup_from(&shop.st, &file).unwrap();
    let listed = shop.st.db.list_backups().unwrap();
    assert_eq!(listed.len(), before + 1);
    assert!(listed.iter().any(|b| b.file_name == placed.file_name));
    let broken = out.path().join("abime.dzpos");
    std::fs::write(&broken, b"not a zip").unwrap();
    assert_eq!(
        data_files::import_backup_from(&shop.st, &broken)
            .unwrap_err()
            .code(),
        "backup_file_invalid"
    );
    let done = data_files::export_all_to(&shop.st, &out.path().join("donnees.zip")).unwrap();
    assert!(done.rows > 0);
    let journal: Vec<(String, String)> = shop
        .st
        .db
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT action, payload FROM audit_log
                 WHERE action IN ('backup.exported', 'backup.imported', 'data.exported')
                 ORDER BY seq",
            )?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap();
    let actions: Vec<&str> = journal.iter().map(|(a, _)| a.as_str()).collect();
    assert_eq!(
        actions,
        ["backup.exported", "backup.imported", "data.exported"]
    );
    let imported: serde_json::Value = serde_json::from_str(&journal[1].1).unwrap();
    assert_eq!(imported["companyName"], "Supérette El Amel");
    let exported: serde_json::Value = serde_json::from_str(&journal[2].1).unwrap();
    assert_eq!(exported["target"], "all");
}

/// A role that keeps the customer file without kridi rights (`credit.view`).
fn customer_desk(shop: &super::testshop::TestShop) -> String {
    let role = users::create_role(
        &shop.st,
        pos_core::auth::RoleInput {
            name_ar: "مكتب الزبائن".into(),
            name_fr: "Accueil clients".into(),
            name_en: "Front desk".into(),
            permissions: vec![Permission::CustomersView, Permission::CustomersManage],
        },
    )
    .unwrap();
    shop.user("Lina", &role.id, "8642", 0)
}

/// Who owes the shop money is kridi data like the balances: without `credit.view` the "with
/// debt" filter of the search and of the customer file is ignored, so the list of debtors does
/// not leak through it.
#[test]
fn the_list_of_debtors_needs_credit_view() {
    let shop = shop();
    shop.owner();
    let karim = customers::create(&shop.st, CustomerInput::named("Karim")).unwrap();
    customers::create(&shop.st, CustomerInput::named("Amine")).unwrap();
    credit::set_opening(&shop.st, karim.id, Money(150_000), None).unwrap();
    let debtors = CustomerQuery {
        with_debt: true,
        ..CustomerQuery::default()
    };
    let names = |query: &CustomerQuery| {
        let mut names: Vec<String> = customers::search(&shop.st, query.clone())
            .unwrap()
            .into_iter()
            .map(|h| h.name)
            .collect();
        names.sort();
        names
    };
    assert_eq!(names(&debtors), ["Karim"]);
    let dir = tempfile::tempdir().unwrap();
    let export = |query: &CustomerQuery| {
        let request = exports::ExportRequest {
            target: exports::ExportTarget::Customers {
                query: query.clone(),
            },
            format: SheetFormat::Csv,
            lang: Lang::Fr,
        };
        exports::export_to(&shop.st, request, &dir.path().join("clients.csv"))
            .unwrap()
            .rows
    };
    assert_eq!(export(&debtors), 1);
    let desk = customer_desk(&shop);
    shop.sign_in(&desk, "8642");
    assert_eq!(names(&debtors), names(&CustomerQuery::default()));
    assert_eq!(names(&debtors), ["Amine", "Karim"]);
    assert_eq!(export(&debtors), 2, "the whole file, not the debtors");
}

/// An import keeps the field guards of the typed commands. The storekeeper (`products.import`,
/// but neither `products.price` nor `reports.profit`) cannot reprice the catalogue or overwrite
/// the CMUP through an upsert, and a customer desk without `credit.view` cannot set limits: the
/// dry run already refuses. A create-only import sets first prices, like `products_create`.
#[test]
fn an_import_cannot_change_prices_costs_or_limits_the_user_may_not() {
    let shop = shop();
    shop.owner();
    let mut input = pos_core::catalog::products::ProductInput::simple(
        "Huile 5 L",
        pos_core::catalog::UNIT_PIECE_ID,
        &shop.rate_id(1_900),
        Money(119_000),
    );
    input.reference = Some("A1".into());
    input.cost_price = Money(80_000);
    let oil = shop.product_with(input);
    let karim = customers::create(&shop.st, CustomerInput::named("Karim")).unwrap();
    let clerk = shop.user("Rachid", STOREKEEPER_ROLE_ID, "1357", 0);
    shop.sign_in(&clerk, "1357");
    let csv = "Référence;Désignation;Prix de vente;Prix d'achat;Prix de gros\r\nA1;Huile;1500,00;900,00;1400,00\r\nA2;Sucre;200,00;150,00;180,00\r\n";
    let token = imports::stage_bytes(&shop.st, csv.as_bytes(), "tarifs.csv")
        .unwrap()
        .token;
    let options = |entity, policy, mapping: &[&str]| ImportOptions {
        entity,
        policy,
        file_name: "tarifs.csv".into(),
        header_row: 0,
        mapping: mapping
            .iter()
            .map(|k| (!k.is_empty()).then(|| (*k).to_owned()))
            .collect(),
        location_id: None,
        count_id: None,
    };
    for (mapping, permission) in [
        (&["reference", "", "sellPrice"][..], "products.price"),
        (&["reference", "", "", "costPrice"][..], "reports.profit"),
        (
            &["reference", "", "", "", "price.wholesale"][..],
            "products.price",
        ),
    ] {
        for policy in [ImportPolicy::Upsert, ImportPolicy::Sync] {
            let refused = imports::dry_run(
                &shop.st,
                token.clone(),
                0,
                options(ImportEntity::Products, policy, mapping),
            )
            .unwrap_err();
            assert_eq!(
                refused,
                pos_core::error::AppError::Permission {
                    permission: permission.into()
                },
                "{mapping:?}"
            );
        }
    }
    let stored = |id: &str| {
        let id = id.to_owned();
        shop.st
            .db
            .read(move |c| pos_core::catalog::products::get(c, &id))
            .unwrap()
    };
    let unchanged = stored(&oil.id);
    assert_eq!(
        (unchanged.sell_price, unchanged.cost_price, unchanged.prices),
        (Money(119_000), Money(80_000), Vec::new()),
        "the stored prices and CMUP are untouched"
    );
    // Create only: the new product takes the file's first prices, the existing one is skipped.
    let first = options(
        ImportEntity::Products,
        ImportPolicy::CreateOnly,
        &["reference", "nameFr", "sellPrice", "costPrice"],
    );
    imports::dry_run(&shop.st, token.clone(), 0, first.clone()).unwrap();
    let job = imports::apply(&shop.st, token, 0, first, &mut |_| {}).unwrap();
    assert_eq!((job.created, job.skipped), (1, 1));
    assert_eq!(stored(&oil.id).sell_price, Money(119_000));

    // Credit limits: kridi data.
    shop.owner();
    let desk = customer_desk(&shop);
    shop.sign_in(&desk, "8642");
    let csv = format!("Code;Nom;Plafond\r\n{};Karim;5000,00\r\n", karim.code);
    let token = imports::stage_bytes(&shop.st, csv.as_bytes(), "clients.csv")
        .unwrap()
        .token;
    let refused = imports::dry_run(
        &shop.st,
        token,
        0,
        options(
            ImportEntity::Customers,
            ImportPolicy::Upsert,
            &["code", "name", "creditLimit"],
        ),
    )
    .unwrap_err();
    assert_eq!(refused.code(), "insufficient_permission");
    shop.owner();
    assert_eq!(
        customers::get(&shop.st, karim.id).unwrap().credit_limit,
        None
    );
}
