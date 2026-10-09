//! End-to-end command flows of the framework, restaurant, pharmacy and butcher packs (Task 19).

use super::lan_rules_tests::{as_till, link};
use super::testshop::{TestShop, shop, shop_in, use_file_printers, wait_for_bytes};
use super::{butcher, packs, pharmacy, restaurant, sales};
use crate::printing::PackSlip;
use pos_core::company::ShopActivity;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::packs::Module;
use pos_core::payments::PaymentInput;
use pos_core::restaurant::bill::PayInput;
use pos_core::restaurant::tickets::{AddLineInput, DiningMode, OpenInput};
use pos_core::sales::CartLine;

fn today(t: &TestShop) -> String {
    pos_core::time::business_date(t.st.now())
        .format("%Y-%m-%d")
        .to_string()
}

fn dine_in(table_id: &str) -> OpenInput {
    OpenInput {
        mode: DiningMode::DineIn,
        table_id: Some(table_id.into()),
        label: None,
        covers: 2,
        customer_id: None,
        delivery_address: None,
        lang: Lang::Fr,
        price_level_id: None,
    }
}

fn line(product_id: &str, pieces: i64) -> AddLineInput {
    AddLineInput {
        cart_line: CartLine::product(product_id, Qty(pieces * 1_000)),
        seat: None,
        course: None,
        modifiers: Vec::new(),
    }
}

fn printed_at(t: &TestShop, kitchen_id: &str) -> Option<i64> {
    let id = kitchen_id.to_owned();
    t.st.db
        .read(move |c| pos_core::restaurant::kitchen::get(c, &id))
        .unwrap()
        .printed_at
}

/// Waits (≤ 10 s) until the print thread marked the kitchen ticket printed.
fn wait_printed(t: &TestShop, kitchen_id: &str) -> i64 {
    for _ in 0..200 {
        if let Some(at) = printed_at(t, kitchen_id) {
            return at;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("kitchen ticket {kitchen_id} never marked printed");
}

#[test]
fn a_restaurant_ticket_goes_to_the_kitchen_and_is_paid_with_its_dining_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let (t, _failures) = shop_in(dir.path());
    let st = &t.st;
    packs::apply_template(st, ShopActivity::FoodService, true).unwrap();
    use_file_printers(st, dir.path(), &["caisse.bin"]);
    let station = restaurant::stations(st)
        .unwrap()
        .into_iter()
        .find(|s| s.code == "CUISINE")
        .unwrap();
    t.open_till(50_000);
    let plat = t.product_in_category("Brochettes", 90_000, &station.category_ids[0]);
    let table = restaurant::tables(st, None).unwrap().remove(0);
    let ticket = restaurant::ticket_open(st, dine_in(&table.id)).unwrap();
    restaurant::ticket_add_line(st, ticket.id.clone(), line(&plat, 2)).unwrap();
    let sent = restaurant::ticket_send(st, ticket.id.clone()).unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].station_id.as_deref(), Some(station.id.as_str()));
    let kitchen = wait_for_bytes(&dir.path().join("caisse.bin"), 64);
    assert!(String::from_utf8_lossy(&kitchen).contains("Brochettes"));
    wait_printed(&t, &sent[0].id);
    let before_receipt = std::fs::read(dir.path().join("caisse.bin")).unwrap().len();
    let paid = restaurant::ticket_pay(
        st,
        PayInput {
            ticket_id: ticket.id.clone(),
            payments: vec![PaymentInput::cash(Money(200_000))],
        },
        true,
    )
    .unwrap();
    assert!(paid.print_queued);
    assert_eq!(paid.paid.sale.document.total_ttc, Money(180_000));
    let receipt = wait_for_bytes(&dir.path().join("caisse.bin"), before_receipt + 64);
    let number = paid.paid.sale.document.number.clone().unwrap();
    assert!(String::from_utf8_lossy(&receipt[before_receipt..]).contains(&number));
    let (loaded, _) =
        crate::printing::load_receipt(&st.db, &paid.paid.sale.document.id, false, None).unwrap();
    let dining = loaded.dining.expect("the receipt carries its dining block");
    assert_eq!(
        dining.place.as_deref(),
        Some(format!("Table {}", table.code).as_str()),
        "the receipt names the table"
    );
    assert_eq!(dining.covers, 2);
}

#[test]
fn a_kitchen_ticket_that_did_not_print_stays_pending_until_its_reprint() {
    let dir = tempfile::tempdir().unwrap();
    let (t, failures) = shop_in(dir.path());
    let st = &t.st;
    packs::apply_template(st, ShopActivity::FoodService, true).unwrap();
    let station = restaurant::stations(st).unwrap().remove(0);
    let plat = t.product_in_category("Chorba", 35_000, &station.category_ids[0]);
    let table = restaurant::tables(st, None).unwrap().remove(0);
    let ticket = restaurant::ticket_open(st, dine_in(&table.id)).unwrap();
    restaurant::ticket_add_line(st, ticket.id.clone(), line(&plat, 1)).unwrap();
    let sent = restaurant::ticket_send(st, ticket.id.clone()).unwrap();
    let failed = failures
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    assert_eq!(
        (failed.document_id.as_str(), failed.slip),
        (sent[0].id.as_str(), Some(PackSlip::Kitchen))
    );
    assert_eq!(failed.code, "printer_not_configured");
    assert_eq!(printed_at(&t, &sent[0].id), None);
    use_file_printers(st, dir.path(), &["caisse.bin"]);
    restaurant::reprint_kitchen(st, sent[0].id.clone()).unwrap();
    let bytes = wait_for_bytes(&dir.path().join("caisse.bin"), 64);
    assert!(String::from_utf8_lossy(&bytes).contains("Chorba"));
    wait_printed(&t, &sent[0].id);
}

#[test]
fn the_addition_prints_on_the_receipt_printer_and_marks_the_table_billed() {
    let dir = tempfile::tempdir().unwrap();
    let (t, _failures) = shop_in(dir.path());
    let st = &t.st;
    packs::apply_template(st, ShopActivity::FoodService, true).unwrap();
    use_file_printers(st, dir.path(), &["caisse.bin"]);
    let station = restaurant::stations(st).unwrap().remove(0);
    let plat = t.product_in_category("Couscous", 60_000, &station.category_ids[0]);
    let table = restaurant::tables(st, None).unwrap().remove(0);
    let ticket = restaurant::ticket_open(st, dine_in(&table.id)).unwrap();
    restaurant::ticket_add_line(st, ticket.id.clone(), line(&plat, 3)).unwrap();
    let bill = restaurant::ticket_prebill(st, ticket.id.clone(), Some(3)).unwrap();
    assert_eq!(bill.shares, vec![Money(60_000); 3]);
    let bytes = wait_for_bytes(&dir.path().join("caisse.bin"), 64);
    assert!(String::from_utf8_lossy(&bytes).contains("Couscous"));
    let board = restaurant::board(st, None).unwrap();
    let state = board
        .iter()
        .find(|s| s.table.id == table.id)
        .map(|s| s.state);
    assert_eq!(state, Some(pos_core::restaurant::floor::TableState::Billed));
}

#[test]
fn the_till_sale_carries_its_pack_extras() {
    let dir = tempfile::tempdir().unwrap();
    let (t, _failures) = shop_in(dir.path());
    let st = &t.st;
    packs::apply_template(st, ShopActivity::Pharmacy, true).unwrap();
    t.open_till(50_000);
    let med = t.controlled_medicine("Tramadol 50 mg", 45_000);
    let cart = pos_core::sales::Cart::new(Lang::Fr, vec![CartLine::product(&med, Qty(1_000))]);
    let without = serde_json::json!({
        "sale": { "cart": cart, "payments": [{ "method": "cash", "amount": 50_000 }] },
        "printReceipt": false
    });
    let refused = sales::create(st, serde_json::from_value(without).unwrap()).unwrap_err();
    assert_eq!(refused.code(), "register_entry_required");
    let request = serde_json::json!({
        "sale": { "cart": cart, "payments": [{ "method": "cash", "amount": 50_000 }] },
        "printReceipt": false,
        "extras": { "prescriptions": [{ "line": 0, "prescriberName": "Dr Benali", "prescriberRef": "CNOM 1234", "prescriptionNo": "0457", "prescriptionDate": "2026-09-27", "patientName": "M. Haddad", "patientRef": null }] }
    });
    let request: sales::SaleRequest = serde_json::from_value(request).unwrap();
    sales::create(st, request).unwrap();
    let entries = pharmacy::register_entries(st, today(&t), today(&t)).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].patient_name, "M. Haddad");
    let old: sales::SaleRequest = serde_json::from_value(serde_json::json!({
        "sale": { "cart": pos_core::sales::Cart::new(Lang::Fr, vec![]), "payments": [] },
        "printReceipt": false
    }))
    .unwrap();
    assert_eq!(
        old.extras,
        pos_core::sales::SaleExtras::default(),
        "M1b's request shape still parses"
    );
}

#[test]
fn a_module_that_is_off_answers_module_disabled() {
    let t = shop();
    let st = &t.st;
    packs::set_modules(st, vec![Module::Pharmacy]).unwrap();
    assert_eq!(
        restaurant::board(st, None).unwrap_err().code(),
        "module_disabled"
    );
    assert_eq!(
        butcher::runs(st, "2026-09-28".into(), "2026-09-28".into())
            .unwrap_err()
            .code(),
        "module_disabled"
    );
    assert_eq!(
        packs::department_keys(st, false).unwrap_err().code(),
        "module_disabled"
    );
    assert!(pharmacy::chifa_prices(st, String::new(), 10).is_ok());
}

#[test]
fn the_report_catalogue_hides_the_reports_of_modules_that_are_off() {
    let t = shop();
    let st = &t.st;
    packs::set_modules(st, Vec::new()).unwrap();
    let kinds = |st: &crate::state::AppState| -> Vec<pos_core::reports::ReportKind> {
        super::reports::catalog(st, Lang::Fr)
            .unwrap()
            .into_iter()
            .map(|r| r.kind)
            .collect()
    };
    let off = kinds(st);
    assert!(!off.contains(&pos_core::reports::ReportKind::ControlledRegister));
    assert!(off.contains(&pos_core::reports::ReportKind::SalesSummary));
    packs::set_modules(st, vec![Module::Pharmacy]).unwrap();
    assert!(kinds(st).contains(&pos_core::reports::ReportKind::ControlledRegister));
}

/// The packs' shared records and the shop-wide yield-run counter are the main PC's;
/// an offline till refuses their writes with `host_required`, and the controlled register
/// (kept on the main PC only) is read there.
#[test]
fn pack_writes_on_an_offline_till_need_the_main_pc() {
    let t = shop();
    let st = &t.st;
    packs::apply_template(st, ShopActivity::FoodService, true).unwrap();
    as_till(&t);
    link(&t, false, None);
    let room = pos_core::restaurant::floor::RoomInput {
        code: "TERRASSE".into(),
        name_ar: "الشرفة".into(),
        name_fr: "Terrasse".into(),
        name_en: "Terrace".into(),
        grid_cols: 4,
        grid_rows: 3,
        sort_order: 2,
    };
    let refused = [
        restaurant::room_save(st, None, room).map(drop),
        packs::set_modules(st, vec![Module::Restaurant]).map(drop),
        restaurant::menu_set(st, "2026-09-28".into(), Vec::new()).map(drop),
        packs::seed_regulated(st).map(drop),
        butcher::run(st, super::tests::probe_yield_run()).map(drop),
        pharmacy::register_entries(st, "2026-09-28".into(), "2026-09-28".into()).map(drop),
    ];
    for result in refused {
        assert_eq!(result.unwrap_err().code(), "host_required");
    }
    assert!(
        restaurant::board(st, None).is_ok(),
        "the floor reads the till's copy"
    );
}

/// On the main PC a till's pack write runs as the till's user: the main PC's roles decide, and
/// what it creates is the main PC's.
#[test]
fn a_pack_service_runs_on_the_main_pc_as_the_tills_user() {
    use pos_server::host::{RpcCaller, RpcHandler};
    let t = shop();
    packs::apply_template(&t.st, ShopActivity::FoodService, true).unwrap();
    let cashier = t.user(
        "Yacine",
        pos_core::auth::CASHIER_ROLE_ID,
        super::testshop::CASHIER_PIN,
        1_000,
    );
    let rpc = crate::lan::rpc::AppRpc::new(std::sync::Arc::downgrade(&t.st));
    let caller = |user: &str| RpcCaller {
        terminal_id: "00000000-0000-7000-8000-0000000000c2".into(),
        terminal_code: "C2".into(),
        user_id: user.into(),
        approvals: Vec::new(),
        idempotency_key: None,
    };
    let room = serde_json::json!({
        "id": null,
        "input": { "code": "TERRASSE", "nameAr": "الشرفة", "nameFr": "Terrasse", "nameEn": "Terrace",
                   "gridCols": 4, "gridRows": 3, "sortOrder": 2 }
    });
    let refused = rpc
        .call(caller(&cashier), "dining.room_save", room.clone())
        .unwrap_err();
    assert_eq!(refused.code(), "insufficient_permission");
    let saved: pos_core::restaurant::floor::Room = serde_json::from_value(
        rpc.call(caller(&t.owner.user_id), "dining.room_save", room)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(saved.code, "TERRASSE");
    let modules: pos_core::packs::PackState = serde_json::from_value(
        rpc.call(
            caller(&t.owner.user_id),
            "packs.set_modules",
            serde_json::json!({ "modules": ["restaurant", "yield"] }),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(modules.modules, vec![Module::Restaurant, Module::Yield]);
}

/// What a pack service answers on the main PC comes back to the till as JSON.
#[test]
fn routed_pack_results_read_back_what_the_main_pc_wrote() {
    fn round_trip<T>(value: &T)
    where
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let back: T = serde_json::from_value(serde_json::to_value(value).unwrap()).unwrap();
        assert_eq!(&back, value);
    }
    let t = shop();
    let st = &t.st;
    round_trip(&packs::apply_template(st, ShopActivity::FoodService, true).unwrap());
    round_trip(&packs::state(st).unwrap());
    round_trip(&restaurant::rooms(st).unwrap());
    round_trip(&restaurant::tables(st, None).unwrap());
    round_trip(&restaurant::groups(st).unwrap());
    round_trip(&restaurant::stations(st).unwrap());
    let set = packs::set_modules(
        st,
        vec![Module::Pharmacy, Module::DepartmentKeys, Module::Yield],
    )
    .unwrap();
    round_trip(&set);
    let general = t.rate_id(1_900);
    let key = packs::save_department(
        st,
        None,
        pos_core::catalog::departments::DepartmentKeyInput {
            name_ar: "متنوع".into(),
            name_fr: "Divers".into(),
            name_en: "Sundries".into(),
            category_id: None,
            tax_rate_id: general,
            unit_id: None,
            color: None,
            sort_order: 1,
        },
    )
    .unwrap();
    round_trip(&key);
    let med = t.controlled_medicine("Tramadol 50 mg", 45_000);
    round_trip(&pharmacy::product_get(st, med).unwrap());
}

/// Kitchen routing: a kitchen ticket prints on its station's printer (its copies),
/// never on the receipt printer; a station whose printer only takes pages (a Windows driver,
/// HW-002) answers `printer_driver_only` and its ticket stays unprinted for `kitchen_reprint`.
#[test]
fn a_kitchen_ticket_prints_on_its_stations_printer_and_a_page_only_printer_is_named() {
    use pos_hw::config::{KitchenRouting, PrintMode, PrinterConfig, StationPrinter};
    use pos_hw::driver::Transport;
    use pos_hw::profile::{PaperWidth, Preset, Profile};
    let dir = tempfile::tempdir().unwrap();
    let (t, failures) = shop_in(dir.path());
    let st = &t.st;
    packs::apply_template(st, ShopActivity::FoodService, true).unwrap();
    use_file_printers(st, dir.path(), &["caisse.bin", "cuisine.bin"]);
    let station = restaurant::stations(st)
        .unwrap()
        .into_iter()
        .find(|s| s.code == "CUISINE")
        .unwrap();
    let route = |printer_id: &str| KitchenRouting {
        stations: vec![StationPrinter {
            station_id: station.id.clone(),
            printer_id: printer_id.into(),
            copies: 2,
            buzzer: false,
        }],
        fallback_printer_id: None,
    };
    let mut config = st.hw.config().unwrap();
    config.kitchen = route("p2");
    st.hw.save(config).unwrap();
    let plat = t.product_in_category("Chorba", 35_000, &station.category_ids[0]);
    let table = restaurant::tables(st, None).unwrap().remove(0);
    let ticket = restaurant::ticket_open(st, dine_in(&table.id)).unwrap();
    restaurant::ticket_add_line(st, ticket.id.clone(), line(&plat, 1)).unwrap();
    let sent = restaurant::ticket_send(st, ticket.id.clone()).unwrap();
    wait_printed(&t, &sent[0].id);
    let kitchen = String::from_utf8_lossy(&std::fs::read(dir.path().join("cuisine.bin")).unwrap())
        .into_owned();
    assert_eq!(
        kitchen.matches("Chorba").count(),
        2,
        "two copies on the station's printer"
    );
    assert!(
        std::fs::read(dir.path().join("caisse.bin")).map_or(true, |b| b.is_empty()),
        "nothing on the receipt printer"
    );

    // the station's printer becomes a Windows printer that only takes pages
    let mut config = st.hw.config().unwrap();
    config.printers.push(PrinterConfig {
        id: "p3".into(),
        name: "HP LaserJet".into(),
        transport: Transport::Windows {
            printer_name: "HP LaserJet".into(),
        },
        profile: Profile::preset(Preset::Generic, PaperWidth::Mm80),
        mode: PrintMode::Driver,
        paper_width_mm: 80,
    });
    config.kitchen = route("p3");
    st.hw.save(config).unwrap();
    restaurant::ticket_add_line(st, ticket.id.clone(), line(&plat, 1)).unwrap();
    let again = restaurant::ticket_send(st, ticket.id.clone()).unwrap();
    let failed = failures
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    assert_eq!(
        (
            failed.document_id.as_str(),
            failed.slip,
            failed.code.as_str()
        ),
        (
            again[0].id.as_str(),
            Some(PackSlip::Kitchen),
            "printer_driver_only"
        )
    );
    assert_eq!(failed.number, None);
    assert_eq!(printed_at(&t, &again[0].id), None);
}

/// One queue: the addition (`PreBill`) prints through pos-hw's layout with
/// the demo mention on the demo shop; a failed one is named as the addition of its table ticket
/// (no number) and kept out of the receipt retries (`dining_ticket_prebill` prints it again).
#[test]
fn a_failed_addition_is_named_by_its_table_ticket_and_never_pending_a_retry() {
    let dir = tempfile::tempdir().unwrap();
    let (t, failures) = shop_in(dir.path());
    let st = &t.st;
    packs::apply_template(st, ShopActivity::FoodService, true).unwrap();
    let station = restaurant::stations(st).unwrap().remove(0);
    let plat = t.product_in_category("Chorba", 35_000, &station.category_ids[0]);
    let table = restaurant::tables(st, None).unwrap().remove(0);
    let ticket = restaurant::ticket_open(st, dine_in(&table.id)).unwrap();
    restaurant::ticket_add_line(st, ticket.id.clone(), line(&plat, 1)).unwrap();
    restaurant::ticket_prebill(st, ticket.id.clone(), None).unwrap();
    let failed = failures
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    assert_eq!(
        (
            failed.document_id.as_str(),
            failed.slip,
            failed.number.clone()
        ),
        (ticket.id.as_str(), Some(PackSlip::PreBill), None)
    );
    assert_eq!(
        st.printer.retry(&ticket.id).unwrap_err().code(),
        "print_not_pending",
        "printed again by dining_ticket_prebill, never by a retry"
    );
}
