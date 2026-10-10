//! Loyalty, vouchers, jewellery, staff, zakat and the completion items through their commands
//! (Task 21): the slips they print, the host services of their writes, the zakat statement's
//! page and the receipt printed through a Windows driver.

use super::lan_rules_tests::{as_till, link};
use super::testshop::{TestShop, file_printer, shop, shop_in, wait_for_bytes};
use super::{completion, jewellery, loyalty, staff};
use crate::print_host::HostPage;
use pos_core::company::ShopActivity;
use pos_core::jewellery::Metal;
use pos_core::lang::Lang;
use pos_core::money::Money;
use pos_core::packs::Module;
use pos_core::payments::PaymentMethod;

/// The owner turns exactly `modules` on.
fn modules(t: &TestShop, modules: &[Module]) {
    let (actor, modules) = (t.owner.actor(), modules.to_vec());
    t.st.db
        .write(move |tx| pos_core::packs::set_modules(tx, &actor, &modules))
        .unwrap();
}

/// The owner enters the nisab of the year (`nisab` centimes) with its source note.
fn enter_nisab(t: &TestShop, nisab: i64, source: &str) {
    let st = &t.st;
    super::settings::set(st, "zakat.nisab".into(), serde_json::json!(nisab)).unwrap();
    super::settings::set(st, "zakat.nisab_source".into(), serde_json::json!(source)).unwrap();
}

/// The receipt printer writes to `<dir>/caisse.bin`.
fn caisse(t: &TestShop, dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("caisse.bin");
    t.st.hw.save(file_printer(&path)).unwrap();
    path
}

/// Karima, a customer with a phone, made by the owner.
fn karima(t: &TestShop) -> String {
    let mut input = pos_core::customers::CustomerInput::named("Karima");
    input.phone = Some("0550123456".into());
    super::customers::create(&t.st, input).unwrap().id
}

#[test]
fn a_gift_voucher_is_sold_and_its_slip_prints() {
    let dir = tempfile::tempdir().unwrap();
    let (t, _failures) = shop_in(dir.path());
    let st = &t.st;
    modules(&t, &[Module::GiftVouchers]);
    let printed = caisse(&t, dir.path());
    t.open_till(50_000);
    let quote =
        loyalty::voucher_quote(st, loyalty::voucher_sale(500_000, PaymentMethod::Cash)).unwrap();
    assert_eq!(
        quote.to_receive,
        Money(505_000),
        "the duty on the cash is charged"
    );
    let done =
        loyalty::voucher_sell(st, loyalty::voucher_sale(500_000, PaymentMethod::Cash)).unwrap();
    assert!(done.print_queued);
    let bytes = String::from_utf8_lossy(&wait_for_bytes(&printed, 64)).into_owned();
    assert!(bytes.contains(&done.voucher.code), "{bytes}");
    assert_eq!(
        loyalty::voucher_get(st, done.voucher.barcode.clone())
            .unwrap()
            .code,
        done.voucher.code,
        "the scanned barcode finds the voucher"
    );
}

/// Task 21 review: a slip that did not print says so in its `print-failed` event, so the
/// window reprints it with `vouchers_reprint` (never a receipt retry or a DUPLICATA).
#[test]
fn a_voucher_slip_that_fails_is_named_a_slip() {
    let dir = tempfile::tempdir().unwrap();
    let (t, failures) = shop_in(dir.path());
    let st = &t.st;
    modules(&t, &[Module::GiftVouchers]);
    t.open_till(50_000);
    let done =
        loyalty::voucher_sell(st, loyalty::voucher_sale(500_000, PaymentMethod::CardCib)).unwrap();
    assert!(done.print_queued);
    let failed = failures
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    assert_eq!(failed.document_id, done.voucher.code, "no printer");
    assert_eq!(failed.slip, Some(crate::printing::PackSlip::Voucher));
    let event = serde_json::to_value(&failed).unwrap();
    assert_eq!(event["slip"], "voucher", "{event}");
    assert_eq!(event["versement"], false);
}

#[test]
fn points_turn_into_a_loyalty_voucher_that_prints() {
    let dir = tempfile::tempdir().unwrap();
    let (t, _failures) = shop_in(dir.path());
    let st = &t.st;
    modules(&t, &[Module::Loyalty, Module::GiftVouchers]);
    let printed = caisse(&t, dir.path());
    let customer = karima(&t);
    let card = loyalty::enrol(st, customer.clone(), None).unwrap();
    let found = loyalty::card_lookup(st, card.code.clone())
        .unwrap()
        .unwrap();
    assert_eq!(found.id, customer);
    let account = loyalty::adjust(st, customer.clone(), 600, "Carte papier".into()).unwrap();
    assert!(account.redeemable);
    let done = loyalty::redeem(st, customer.clone(), 500).unwrap();
    assert_eq!(done.voucher.initial_amount, Money(50_000));
    let bytes = String::from_utf8_lossy(&wait_for_bytes(&printed, 64)).into_owned();
    assert!(bytes.contains(&done.voucher.code), "{bytes}");
    assert_eq!(loyalty::account(st, customer.clone()).unwrap().balance, 100);
    assert_eq!(loyalty::entries(st, customer).unwrap().len(), 2);
}

#[test]
fn zakat_and_jewellery_run_through_their_commands() {
    let t = shop();
    let st = &t.st;
    let (actor, pack) = (t.owner.actor(), st.datapack.clone());
    st.db
        .write(move |tx| {
            pos_core::packs::apply_template(
                tx,
                &actor,
                &pack,
                ShopActivity::Jewellery,
                Lang::Fr,
                true,
            )
        })
        .unwrap();
    modules(&t, &[Module::Jewellery, Module::Zakat]);
    jewellery::set_metal_price(st, Metal::Gold, 750, Money(1_450_000)).unwrap();
    assert_eq!(jewellery::board(st).unwrap().len(), 1);
    assert_eq!(jewellery::history(st, Metal::Gold, 750).unwrap().len(), 1);
    // Nothing is computed or saved until the shop enters the nisab itself.
    for refused in [
        staff::preview_zakat(st, staff::zakat_input(Money(300_000_000))).map(drop),
        staff::save_zakat(st, staff::zakat_input(Money(300_000_000))).map(drop),
    ] {
        assert_eq!(refused.unwrap_err().code(), "zakat_nisab_missing");
    }
    enter_nisab(&t, 200_000_000, "MARW, 1447");
    let preview = staff::preview_zakat(st, staff::zakat_input(Money(300_000_000))).unwrap();
    assert!(!preview.below_nisab);
    assert_eq!(preview.nisab, Money(200_000_000));
    let saved = staff::save_zakat(st, staff::zakat_input(Money(300_000_000))).unwrap();
    assert!(saved.number.starts_with("ZK"), "{}", saved.number);
    assert_eq!(staff::list_zakat(st).unwrap().len(), 1);
    // A later note does not change what the saved statement prints.
    enter_nisab(&t, 200_000_000, "MARW, 1448");
    let page = staff::zakat_page(st, saved.id.clone(), Lang::Ar).unwrap();
    let HostPage::ZakatStatement(print) = page else {
        panic!("not a zakat statement page");
    };
    assert!(print.rtl);
    assert_eq!(print.number, saved.number);
    assert_eq!(print.due, saved.preview.zakat_due);
    assert_eq!(
        (print.nisab, print.source.as_str()),
        (Money(200_000_000), "النصاب: MARW, 1447")
    );
    let cashier = t.cashier();
    t.owner();
    let dup =
        completion::duplicate_user(st, cashier.user_id, "Sara".into(), "2468".into()).unwrap();
    assert_eq!(dup.display_name, "Sara");
}

#[test]
fn payment_types_and_the_baridpay_qr() {
    let t = shop();
    let st = &t.st;
    let types = completion::list_types(st, false).unwrap();
    assert!(types.iter().any(|p| p.code == "ccp_debit"), "{types:?}");
    let input = pos_core::payment_types::PaymentTypeInput {
        code: "bna_tpe".into(),
        name_ar: String::new(),
        name_fr: "TPE BNA".into(),
        name_en: String::new(),
        base_method: PaymentMethod::CardCib,
        requires_reference: true,
        sort_order: 3,
    };
    let made = completion::save_type(st, None, input.clone()).unwrap();
    let renamed = pos_core::payment_types::PaymentTypeInput {
        name_fr: "TPE BNA Bab Ezzouar".into(),
        ..input
    };
    let changed = completion::save_type(st, Some(made.id.clone()), renamed).unwrap();
    assert_eq!(changed.name_fr, "TPE BNA Bab Ezzouar");
    completion::archive_type(st, made.id).unwrap();
    assert!(completion::baridpay_qr(st).unwrap().is_none());
    super::settings::set(
        st,
        "payments.baridpay_qr".into(),
        serde_json::json!("BARIDIMOB:00799999000123456789"),
    )
    .unwrap();
    let qr = completion::baridpay_qr(st).unwrap().unwrap();
    assert_eq!(qr.modules.len(), (qr.size * qr.size) as usize);
}

/// The pack writes and the shop-wide counters are the main PC's; an offline till
/// refuses them with `host_required` before writing anything.
#[test]
fn pack_writes_on_an_offline_till_need_the_main_pc() {
    let t = shop();
    let st = &t.st;
    let customer = karima(&t);
    t.open_till(10_000);
    as_till(&t);
    link(&t, false, None);
    // Cash for these is refused before the link is asked (`a_till_takes_no_cash_for_a_host_service`).
    let refused = [
        loyalty::voucher_sell(st, loyalty::voucher_sale(100_000, PaymentMethod::CardCib)).map(drop),
        loyalty::enrol(st, customer.clone(), None).map(drop),
        loyalty::redeem(st, customer.clone(), 500).map(drop),
        loyalty::adjust(st, customer, 10, "Correction".into()).map(drop),
        staff::advance(
            st,
            pos_core::staff::AdvanceInput {
                user_id: t.owner.user_id.clone(),
                amount: Money(100_000),
                method: PaymentMethod::Transfer,
                from_till: false,
                note: None,
            },
        )
        .map(drop),
        staff::save_zakat(st, staff::zakat_input(Money(300_000_000))).map(drop),
        jewellery::set_metal_price(st, Metal::Gold, 750, Money(1_450_000)).map(drop),
    ];
    for (i, result) in refused.into_iter().enumerate() {
        assert_eq!(
            result.unwrap_err().code(),
            "host_required",
            "write {i} on an offline till"
        );
    }
}

/// On a till, the cash of a
/// gift voucher, of an advance from the drawer and of a repayment goes to the main PC like any
/// other method (it books the money on this till's session and this till records it in its own
/// drawer: `lan::pack_cash_tests`); with no main PC answering, each is `host_required`, link up
/// or down, and this till's drawer is untouched.
#[test]
fn a_tills_pack_cash_goes_to_the_main_pc() {
    let t = shop();
    let st = &t.st;
    t.open_till(10_000);
    as_till(&t);
    let owner = t.owner.user_id.clone();
    let advance = |method: PaymentMethod, from_till: bool| pos_core::staff::AdvanceInput {
        user_id: owner.clone(),
        amount: Money(100_000),
        method,
        from_till,
        note: None,
    };
    let writes = || {
        [
            loyalty::voucher_sell(st, loyalty::voucher_sale(100_000, PaymentMethod::Cash))
                .map(drop),
            staff::advance(st, advance(PaymentMethod::Cash, true)).map(drop),
            staff::repay(st, owner.clone(), Money(50_000), PaymentMethod::Cash, None).map(drop),
            loyalty::voucher_sell(st, loyalty::voucher_sale(100_000, PaymentMethod::CardCib))
                .map(drop),
            staff::advance(st, advance(PaymentMethod::Transfer, false)).map(drop),
            staff::repay(
                st,
                owner.clone(),
                Money(50_000),
                PaymentMethod::Baridimob,
                None,
            )
            .map(drop),
        ]
    };
    for online in [true, false] {
        link(&t, online, None);
        for (i, result) in writes().into_iter().enumerate() {
            assert_eq!(
                result.unwrap_err().code(),
                "host_required",
                "write {i} goes to the main PC (online: {online})"
            );
        }
    }
    let moved: i64 = st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM cash_movements WHERE kind <> 'float'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(moved, 0, "nothing moved this till's drawer");
}

/// What a host service answers comes back to the till as JSON: every routed input and result
/// reads back what it carried.
#[test]
fn routed_pack_values_read_back() {
    fn back<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
        value: T,
    ) {
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(serde_json::from_value::<T>(json).unwrap(), value);
    }
    let t = shop();
    let st = &t.st;
    let customer = karima(&t);
    back(loyalty::voucher_sale(100_000, PaymentMethod::CardCib));
    back(loyalty::enrol(st, customer.clone(), None).unwrap());
    back(loyalty::adjust(st, customer.clone(), 600, "Carte papier".into()).unwrap());
    back(loyalty::redeem(st, customer, 500).unwrap().voucher);
    back(staff::zakat_input(Money(300_000_000)));
    enter_nisab(&t, 229_000_000, "");
    let saved = staff::save_zakat(st, staff::zakat_input(Money(300_000_000))).unwrap();
    back(saved);
    let profile = pos_core::staff::ProfileInput {
        phone: Some("0661223344".into()),
        hire_date: Some("2026-01-15".into()),
        monthly_salary: Some(Money(4_500_000)),
        commission_bp: 100,
        note: None,
    };
    back(profile.clone());
    back(staff::profile_set(st, t.owner.user_id.clone(), profile).unwrap());
    let advance = pos_core::staff::AdvanceInput {
        user_id: t.owner.user_id.clone(),
        amount: Money(100_000),
        method: PaymentMethod::Cash,
        from_till: false,
        note: Some("Aïd".into()),
    };
    back(advance.clone());
    back(staff::advance(st, advance).unwrap());
}

/// LOY-001: a voucher past its validity is written off at the next start of the main PC (its
/// liability leaves the report), once.
#[test]
fn a_voucher_past_its_validity_expires_at_the_next_start() {
    let t = shop();
    let st = &t.st;
    // Noon in Algiers (UTC+1) on 2025-03-01, then on 2026-06-01.
    let (march_2025, june_2026) = (1_740_826_800_000, 1_780_311_600_000);
    st.clock.pin(march_2025);
    let sold =
        loyalty::voucher_sell(st, loyalty::voucher_sale(200_000, PaymentMethod::CardCib)).unwrap();
    assert_eq!(sold.voucher.expires_on, "2026-03-01");
    crate::state::run_expiries(st);
    let code = sold.voucher.code.clone();
    assert_eq!(
        loyalty::voucher_get(st, code.clone()).unwrap().status,
        pos_core::gift_vouchers::VoucherStatus::Active
    );
    st.clock.pin(june_2026);
    crate::state::run_expiries(st);
    crate::state::run_expiries(st);
    let expired = loyalty::voucher_get(st, code).unwrap();
    assert_eq!(
        (expired.status, expired.balance),
        (pos_core::gift_vouchers::VoucherStatus::Expired, Money::ZERO)
    );
    assert_eq!(
        t.count("SELECT COUNT(*) FROM audit_log WHERE action = 'vouchers.expired'"),
        1
    );
}

/// The pages a test print host printed: (template, printer, page width in mm).
#[derive(Default)]
struct Pages {
    printed: std::sync::Mutex<Vec<(String, Option<String>, u32)>>,
    last: std::sync::Mutex<Option<String>>,
}

impl crate::print_host::PrintSurface for Pages {
    fn render(
        &self,
        job: crate::print_host::HostJob,
    ) -> Result<crate::print_host::Rendered, pos_core::error::AppError> {
        let template = serde_json::to_value(&job.page).unwrap()["template"]
            .as_str()
            .unwrap()
            .to_owned();
        *self.last.lock().unwrap() = Some(template);
        Ok(crate::print_host::Rendered { height_px: 900 })
    }
    fn print(
        &self,
        setup: &crate::print_host::PageSetup,
        printer: Option<&str>,
        _copies: u32,
    ) -> Result<crate::print_host::PrintStatus, pos_core::error::AppError> {
        let template = self.last.lock().unwrap().clone().unwrap_or_default();
        self.printed
            .lock()
            .unwrap()
            .push((template, printer.map(str::to_owned), setup.width_mm));
        Ok(crate::print_host::PrintStatus::Printed)
    }
    fn pdf(
        &self,
        _setup: &crate::print_host::PageSetup,
        _path: &std::path::Path,
    ) -> Result<(), pos_core::error::AppError> {
        Ok(())
    }
    fn close_if_idle(&self, _: std::time::Duration) {}
}

/// HW-002: a receipt whose thermal print failed prints once as a page of its roll through the
/// Windows driver, journaled as the original; after that it is a reprint's (a DUPLICATA).
#[test]
fn a_failed_receipt_prints_once_as_a_page_of_its_roll() {
    let dir = tempfile::tempdir().unwrap();
    let (t, failures) = shop_in(dir.path());
    let st = &t.st;
    let milk = t.product("Lait", 10_000, 900, None).id;
    t.stock_in(&milk, 2);
    t.open_till(10_000);
    let sale = pos_core::sales::SaleInput {
        cart: pos_core::sales::Cart::new(
            Lang::Fr,
            vec![pos_core::sales::CartLine::product(
                &milk,
                pos_core::money::Qty(1_000),
            )],
        ),
        payments: vec![pos_core::payments::PaymentInput::cash(Money(10_000))],
    };
    let done = super::sales::create(
        st,
        super::sales::SaleRequest {
            sale,
            print_receipt: true,
            extras: Default::default(),
        },
    )
    .unwrap();
    let id = done.sale.document.id.clone();
    let failed = failures
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    assert_eq!(failed.document_id, id, "no printer: the receipt is pending");
    let pages = std::sync::Arc::new(Pages::default());
    let host = crate::print_host::PrintHost::new(
        pages.clone(),
        std::sync::Arc::new(crate::print_host::HostChannel::new()),
    );
    let refused = super::print::print_receipt_roll(st, &host, id.clone(), "HP".into(), 70);
    assert_eq!(refused.unwrap_err().code(), "value_invalid", "58 or 80 mm");
    let printed =
        super::print::print_receipt_roll(st, &host, id.clone(), "HP LaserJet".into(), 58).unwrap();
    assert!(!printed.duplicate);
    assert_eq!(
        pages.printed.lock().unwrap().as_slice(),
        [(
            "receipt_roll".to_owned(),
            Some("HP LaserJet".to_owned()),
            58
        )]
    );
    let journaled = st
        .db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'document.printed' AND entity_id = ?1",
                [&id],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap();
    assert_eq!(journaled, 1);
    let again = super::print::print_receipt_roll(
        st,
        &host,
        done.sale.document.id.clone(),
        "HP LaserJet".into(),
        58,
    );
    assert_eq!(again.unwrap_err().code(), "print_not_pending");
    assert_eq!(
        st.printer.retry(&done.sale.document.id).unwrap_err().code(),
        "print_not_pending",
        "the original is out: no second one"
    );
}
