//! Printing after the sale, reprints, the drawer, hardware settings, documents and history.

use super::testshop::{file_printer, shop, shop_in, wait_for_bytes};
use super::{audit as audit_cmd, documents, hardware};
use crate::printing::PrintRequest;
use pos_core::audit::AuditFilter;
use pos_core::documents::DocumentQuery;
use pos_core::lang::Lang;
use std::time::Duration;

#[test]
fn a_failed_print_never_touches_the_sale_and_can_be_retried_once() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, failures) = shop_in(dir.path());
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, None);
    shop.open_till(100_000);
    let sale = shop.sell(&milk.id, 2, 50_000);
    assert!(shop.st.printer.enqueue(PrintRequest {
        document_id: sale.document.id.clone(),
        receipt: true,
        cash_moved: true,
        versement: false,
        slip: None,
        layout: None,
    }));
    let failed = failures.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(failed.document_id, sale.document.id);
    assert_eq!(failed.code, "printer_not_configured");
    assert_eq!(failed.number, sale.document.number);
    assert!(
        !failed.versement,
        "a ticket, reprinted with documents_reprint"
    );
    assert!(
        documents::get(&shop.st, sale.document.id.clone()).is_ok(),
        "the sale stands"
    );
    // plug the printer in, retry: the original prints, once
    let out = dir.path().join("receipt.bin");
    shop.st.hw.save(file_printer(&out)).unwrap();
    hardware::retry_print(&shop.st, sale.document.id.clone()).unwrap();
    let bytes = wait_for_bytes(&out, 100);
    assert!(
        !bytes.windows(9).any(|w| w == b"DUPLICATA"),
        "the retry is the original"
    );
    assert_eq!(
        hardware::retry_print(&shop.st, sale.document.id)
            .unwrap_err()
            .code(),
        "print_not_pending"
    );
}

/// A DUPLICATA settles a failed print: the retry can no longer print an unmarked original after
/// it (the toast then falls back to another DUPLICATA).
#[test]
fn a_duplicate_clears_the_pending_retry() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, failures) = shop_in(dir.path());
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, None);
    shop.open_till(100_000);
    let sale = shop.sell(&milk.id, 1, 15_000);
    assert!(shop.st.printer.enqueue(PrintRequest {
        document_id: sale.document.id.clone(),
        receipt: true,
        cash_moved: false,
        versement: false,
        slip: None,
        layout: None,
    }));
    let failed = failures.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(failed.document_id, sale.document.id);
    let out = dir.path().join("receipt.bin");
    shop.st.hw.save(file_printer(&out)).unwrap();
    documents::reprint(&shop.st, sale.document.id.clone(), None).unwrap();
    assert!(
        std::fs::read(&out)
            .unwrap()
            .windows(9)
            .any(|w| w == b"DUPLICATA")
    );
    assert_eq!(
        hardware::retry_print(&shop.st, sale.document.id)
            .unwrap_err()
            .code(),
        "print_not_pending"
    );
}

#[test]
fn a_reprint_is_marked_duplicate_and_journaled() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, _) = shop_in(dir.path());
    let out = dir.path().join("receipt.bin");
    shop.st.hw.save(file_printer(&out)).unwrap();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, None);
    shop.open_till(100_000);
    let sale = shop.sell(&milk.id, 1, 15_000);
    shop.cashier();
    let outcome = documents::reprint(&shop.st, sale.document.id.clone(), Some(Lang::En)).unwrap();
    assert_eq!(outcome.printer, "Caisse 1");
    let bytes = std::fs::read(&out).unwrap();
    assert!(
        bytes.windows(9).any(|w| w == b"DUPLICATE"),
        "English duplicate marker"
    );
    shop.owner();
    let history = audit_cmd::history(
        &shop.st,
        AuditFilter {
            action_prefix: Some("document.".into()),
            ..AuditFilter::default()
        },
    )
    .unwrap();
    assert_eq!(history.rows[0].action, "document.reprinted");
    let storekeeper = shop.user("Karim", pos_core::auth::STOREKEEPER_ROLE_ID, "5173", 0);
    shop.sign_in(&storekeeper, "5173");
    assert_eq!(
        documents::reprint(&shop.st, sale.document.id, None)
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
}

/// HW-002 (Task 18): a receipt printer whose Windows driver only takes pages gets no ESC/POS.
/// The `print-failed` event names the printer and its roll width, and the original stays
/// pending, so the window prints it as a page (`documents_print_receipt_roll`).
#[test]
fn a_page_only_receipt_printer_hands_the_receipt_to_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, failures) = shop_in(dir.path());
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, None);
    shop.open_till(100_000);
    let sale = shop.sell(&milk.id, 1, 15_000);
    let mut config = file_printer(&dir.path().join("receipt.bin"));
    config.printers[0].transport = pos_hw::driver::Transport::Windows {
        printer_name: "POS-58 (GDI)".into(),
    };
    config.printers[0].mode = pos_hw::config::PrintMode::Driver;
    config.printers[0].paper_width_mm = 58;
    shop.st.hw.save(config).unwrap();
    assert!(shop.st.printer.enqueue(PrintRequest {
        document_id: sale.document.id.clone(),
        receipt: true,
        cash_moved: false,
        versement: false,
        slip: None,
        layout: None,
    }));
    let failed = failures.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(failed.document_id, sale.document.id);
    assert_eq!(failed.code, "printer_driver_only");
    assert_eq!(
        (failed.printer_name.as_deref(), failed.paper_width_mm),
        (Some("POS-58 (GDI)"), Some(58))
    );
    assert!(shop.st.printer.is_pending(&sale.document.id).unwrap());
}

#[test]
fn the_drawer_opens_only_with_an_open_session_and_is_journaled() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, _) = shop_in(dir.path());
    let out = dir.path().join("drawer.bin");
    shop.st.hw.save(file_printer(&out)).unwrap();
    shop.cashier();
    let input = || hardware::NoSaleInput {
        reason: hardware::NoSaleReason::ChangeRequest,
        note: None,
    };
    assert_eq!(
        hardware::open_drawer(&shop.st, input()).unwrap_err().code(),
        "cash_session_closed"
    );
    shop.open_till(100_000);
    hardware::open_drawer(&shop.st, input()).unwrap();
    assert_eq!(
        std::fs::read(&out).unwrap(),
        [0x1B, b'@', 0x1B, b'p', 0, 25, 250]
    );
    let other = hardware::NoSaleInput {
        reason: hardware::NoSaleReason::Other,
        note: None,
    };
    assert_eq!(
        hardware::open_drawer(&shop.st, other).unwrap_err().code(),
        "required",
        "'other' needs a note"
    );
    shop.owner();
    let rows = audit_cmd::history(
        &shop.st,
        AuditFilter {
            action_prefix: Some("drawer.".into()),
            ..AuditFilter::default()
        },
    )
    .unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert_eq!(rows.rows[0].payload["reason"], "change_request");
}

#[test]
fn hardware_settings_need_settings_hardware_and_are_journaled() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, _) = shop_in(dir.path());
    let config = file_printer(&dir.path().join("x.bin"));
    shop.cashier();
    assert!(
        hardware::get(&shop.st).is_ok(),
        "the till reads the scanner layout and receipt options"
    );
    assert_eq!(
        hardware::save(&shop.st, config.clone()).unwrap_err().code(),
        "insufficient_permission"
    );
    assert_eq!(
        hardware::discover_devices(&shop.st).unwrap_err().code(),
        "insufficient_permission"
    );
    shop.owner();
    assert_eq!(hardware::save(&shop.st, config.clone()).unwrap(), config);
    let rows = audit_cmd::history(
        &shop.st,
        AuditFilter {
            action_prefix: Some("hardware.".into()),
            ..AuditFilter::default()
        },
    )
    .unwrap();
    assert_eq!(rows.rows[0].action, "hardware.updated");
    let mut broken = config;
    broken.receipt_printer_id = Some("ghost".into());
    let error = hardware::save(&shop.st, broken).unwrap_err().payload();
    assert_eq!(
        (error.code.as_str(), error.field.as_deref()),
        ("printer_setting_invalid", Some("receiptPrinterId"))
    );
    let outcome = hardware::test_print(
        &shop.st,
        shop.st.hw.config().unwrap().printers[0].clone(),
        Lang::Ar,
    )
    .unwrap();
    assert!(outcome.raster);
}

#[test]
fn documents_are_listed_found_and_verified() {
    let shop = shop();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, None);
    shop.cashier();
    shop.open_till(100_000);
    let sale = shop.sell(&milk.id, 1, 20_000);
    let number = sale.document.number.clone().unwrap();
    let list = documents::list(&shop.st, DocumentQuery::default()).unwrap();
    assert_eq!(list.len(), 1);
    let found = documents::find(&shop.st, number.to_lowercase())
        .unwrap()
        .unwrap();
    assert_eq!(found.id, sale.document.id);
    assert!(
        found.lines.iter().all(|l| l.cost_price.0 == 0),
        "costs hidden from the cashier"
    );
    assert_eq!(
        documents::verify_chain(&shop.st).unwrap_err().code(),
        "insufficient_permission"
    );
    shop.owner();
    let reports = documents::verify_chain(&shop.st).unwrap();
    assert!(reports.iter().all(|r| r.first_broken_seq.is_none()));
}

#[test]
fn a_products_history_lists_its_changes() {
    let shop = shop();
    let oil = shop.product("Huile Elio 5 L", 65_000, 1_900, None);
    let levels = super::catalog::price_levels_list(&shop.st).unwrap();
    let wholesale = levels.iter().find(|l| !l.is_default).unwrap().id.clone();
    super::products::set_price(
        &shop.st,
        oil.id.clone(),
        wholesale,
        pos_core::money::Money(60_000),
    )
    .unwrap();
    let page = audit_cmd::history(
        &shop.st,
        AuditFilter {
            entity_type: Some("product".into()),
            entity_id: Some(oil.id),
            ..AuditFilter::default()
        },
    )
    .unwrap();
    let actions: Vec<&str> = page.rows.iter().map(|r| r.action.as_str()).collect();
    assert_eq!(actions, ["product.price_changed", "product.created"]);
    assert_eq!(
        page.rows[1].payload["after"]["costPrice"],
        serde_json::json!(39_000),
        "the owner sees the cost"
    );
    // An auditor without reports.profit reads the same history without any cost in it.
    let role = super::users::create_role(
        &shop.st,
        pos_core::auth::RoleInput {
            name_ar: "مدقق".into(),
            name_fr: "Contrôleur".into(),
            name_en: "Auditor".into(),
            permissions: vec![
                pos_core::auth::Permission::AuditView,
                pos_core::auth::Permission::ProductsView,
            ],
        },
    )
    .unwrap();
    let auditor = shop.user("Nora", &role.id, "8264", 0);
    shop.sign_in(&auditor, "8264");
    for page in [
        audit_cmd::history(
            &shop.st,
            AuditFilter {
                entity_type: Some("product".into()),
                ..AuditFilter::default()
            },
        )
        .unwrap(),
        audit_cmd::list(&shop.st, pos_core::audit::AuditQuery::default()).unwrap(),
    ] {
        assert!(!page.rows.is_empty());
        for row in &page.rows {
            let text = row.payload.to_string();
            assert!(
                !text.contains("costPrice") && !text.contains("costAfter"),
                "{text}"
            );
        }
    }
    shop.cashier();
    assert_eq!(
        audit_cmd::history(&shop.st, AuditFilter::default())
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
}

#[test]
fn the_shop_name_falls_back_to_the_name_the_shop_has() {
    let shop = shop();
    let name = |lang| crate::printing::shop_name(&shop.st.db, lang).unwrap();
    assert_eq!(name(Lang::Fr), "Supérette El Amel");
    assert_eq!(name(Lang::Ar), "Supérette El Amel", "no Arabic name yet");
}

/// Pack slips on `PackSlip`: a repair intake slip or a deposit
/// receipt that did not print is named by its own number and its kind, so the window titles it
/// and reprints it from the kept layout once (`hardware_retry_print`), never as a DUPLICATA.
#[test]
fn a_failed_pack_slip_is_named_by_its_number_and_kind_and_retried_from_its_layout() {
    use crate::printing::PackSlip;
    use pos_hw::layout::{Align, Layout, Row};
    let dir = tempfile::tempdir().unwrap();
    let (shop, failures) = shop_in(dir.path());
    let layout = Layout {
        rtl: false,
        rows: vec![Row::Text {
            text: "AC2026-000004".into(),
            align: Align::Center,
            large: true,
            bold: true,
        }],
    };
    assert!(shop.st.printer.enqueue(PrintRequest::layout(
        PackSlip::Deposit,
        "AC2026-000004",
        layout
    )));
    let failed = failures.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(
        (
            failed.document_id.as_str(),
            failed.number.as_deref(),
            failed.slip
        ),
        (
            "AC2026-000004",
            Some("AC2026-000004"),
            Some(PackSlip::Deposit)
        )
    );
    assert_eq!(
        serde_json::to_value(&failed).unwrap()["slip"],
        serde_json::json!("deposit")
    );
    assert!(
        !shop.st.printer.is_pending("AC2026-000004").unwrap(),
        "a slip is no receipt pending a DUPLICATA"
    );
    shop.st.printer.retry("AC2026-000004").unwrap();
    let again = failures.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(
        again.slip,
        Some(PackSlip::Deposit),
        "the retry keeps its kind"
    );
    shop.st.printer.retry("AC2026-000004").unwrap();
    failures.recv_timeout(Duration::from_secs(10)).unwrap();
}
