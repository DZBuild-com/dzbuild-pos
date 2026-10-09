//! The licence in the running app: the gate over every command, the start's check that heals after
//! a restore, an expired trial in the middle of a shift, the demo shop, the seller's contact.

use crate::license::{self, LicenseEnv, LicenseRuntime, NEVER_OPEN, OPEN};
use crate::state::{AppPaths, AppState};
use pos_core::audit::Actor;
use pos_core::time::now_ms;
use pos_license::codes;
use pos_license::payload::DAY_MS;
use pos_license::state::LicenseState;
use pos_license::testkit;
use std::collections::BTreeSet;
use std::sync::Arc;

fn activate(st: &AppState, key: &str) -> LicenseState {
    st.license
        .activate(&st.db, &st.actor(None), st.now(), key)
        .unwrap()
}

#[test]
fn every_open_command_exists_and_money_and_stock_writes_are_never_open() {
    for name in OPEN {
        assert!(
            crate::COMMANDS.contains(name),
            "{name} is in OPEN but is not a command"
        );
    }
    for name in NEVER_OPEN {
        assert!(
            crate::COMMANDS.contains(name),
            "{name} is in NEVER_OPEN but is not a command"
        );
        assert!(
            !OPEN.contains(name),
            "{name} sells, moves money or stock, or changes the shop: never open"
        );
    }
    let unique: BTreeSet<&str> = OPEN.iter().copied().collect();
    assert_eq!(unique.len(), OPEN.len(), "each command listed once");
}

/// Every command is classified on purpose, in exactly one list: a command added later fails
/// here until someone decides whether read-only mode keeps it.
#[test]
fn every_command_is_classified_in_exactly_one_list() {
    let open: BTreeSet<&str> = OPEN.iter().copied().collect();
    let never: BTreeSet<&str> = NEVER_OPEN.iter().copied().collect();
    assert_eq!(never.len(), NEVER_OPEN.len(), "each command listed once");
    for name in crate::COMMANDS {
        let lists = usize::from(open.contains(name)) + usize::from(never.contains(name));
        assert_eq!(
            lists, 1,
            "{name}: list it in license::OPEN or in license::NEVER_OPEN, exactly once"
        );
    }
    assert_eq!(open.len() + never.len(), crate::COMMANDS.len());
}

#[test]
fn the_gate_refuses_every_write_without_a_licence_and_opens_with_one() {
    let st = AppState::for_tests();
    assert_eq!(st.license.state(st.now()), LicenseState::Unlicensed);
    for name in crate::COMMANDS {
        let verdict = license::verdict(name, Some(&st));
        if OPEN.contains(name) {
            assert!(
                verdict.is_ok(),
                "{name} must keep working in read-only mode"
            );
        } else {
            assert_eq!(verdict.unwrap_err().code(), codes::REQUIRED, "{name}");
        }
    }
    let state = activate(&st, &testkit::lifetime(&testkit::device(), now_ms()));
    assert!(state.allows_writes(), "{state:?}");
    for name in crate::COMMANDS {
        assert!(
            license::verdict(name, Some(&st)).is_ok(),
            "{name} with a lifetime licence"
        );
    }
}

#[test]
fn an_expired_trial_reads_expired_at_once_and_the_gate_says_so() {
    let st = AppState::for_tests();
    let now = now_ms();
    activate(&st, &testkit::trial(&testkit::device(), now - DAY_MS));
    assert!(license::verdict("sales_create", Some(&st)).is_ok());
    // the trial ends in the middle of a shift: no full check has run since
    st.clock.pin(now + 6 * DAY_MS + 1);
    for gated in [
        "sales_create",
        "returns_create",
        "documents_validate",
        "cash_open_session",
        "purchases_receive",
        "stock_adjust",
    ] {
        assert_eq!(
            license::verdict(gated, Some(&st)).unwrap_err().code(),
            codes::EXPIRED,
            "{gated}"
        );
    }
    for open in [
        "cash_close_session",
        "cash_print_z",
        "reports_run",
        "backup_now",
        "fiscal_export_archive",
        "auth_login",
        "data_export_all",
        "documents_reprint",
    ] {
        assert!(
            license::verdict(open, Some(&st)).is_ok(),
            "{open} keeps working"
        );
    }
    // the clock set back afterwards changes nothing: the newest time seen stays
    st.clock.pin(now);
    assert_eq!(
        license::verdict("sales_create", Some(&st))
            .unwrap_err()
            .code(),
        codes::EXPIRED
    );
}

/// Final review: the clock set back in the middle of a trial (to keep it from ending) stops the
/// writes at once, without waiting for the 10-minute check; set right, they work again.
#[test]
fn a_clock_set_back_mid_trial_refuses_sales_until_the_date_is_set() {
    let st = AppState::for_tests();
    let now = now_ms();
    activate(&st, &testkit::trial(&testkit::device(), now - DAY_MS));
    st.clock.pin(now + 3 * DAY_MS);
    assert!(license::verdict("sales_create", Some(&st)).is_ok());
    st.clock.pin(now + DAY_MS);
    for gated in ["sales_create", "returns_create", "cash_open_session"] {
        assert_eq!(
            license::verdict(gated, Some(&st)).unwrap_err().code(),
            codes::CLOCK_BEHIND,
            "{gated}"
        );
    }
    assert!(
        license::verdict("reports_run", Some(&st)).is_ok(),
        "the books stay open"
    );
    st.clock.pin(now + 3 * DAY_MS + 60_000);
    assert!(license::verdict("sales_create", Some(&st)).is_ok());
    // a lifetime licence is never stopped by the clock
    activate(&st, &testkit::lifetime(&testkit::device(), now));
    st.clock.pin(now);
    assert!(license::verdict("sales_create", Some(&st)).is_ok());
}

#[test]
fn the_demo_shop_needs_no_licence() {
    let db = pos_core::db::Db::open_memory().unwrap();
    let actor = Actor::system(None);
    let runtime = LicenseRuntime::open(&db, LicenseEnv::for_tests(), true, &actor, now_ms());
    assert_eq!(runtime.state(now_ms()), LicenseState::Demo);
    assert!(runtime.check(now_ms()).is_ok());
    let key = testkit::lifetime(&testkit::device(), now_ms());
    assert_eq!(
        runtime
            .activate(&db, &actor, now_ms(), &key)
            .unwrap_err()
            .code(),
        "demo_not_allowed"
    );
}

#[test]
fn before_the_database_opens_only_open_commands_pass() {
    assert_eq!(
        license::verdict("sales_create", None).unwrap_err().code(),
        codes::REQUIRED
    );
    for name in [
        "app_startup_error",
        "app_open_folder",
        "recovery_list",
        "recovery_restore",
        "recovery_rejoin",
    ] {
        assert!(
            license::verdict(name, None).is_ok(),
            "{name}: the fatal screen needs it"
        );
    }
}

#[test]
fn a_file_state_checks_its_licence_at_start_and_heals_after_a_restore() {
    let dir = tempfile::tempdir().unwrap();
    let open =
        || Arc::new(AppState::open(&AppPaths::new(dir.path()), crate::printing::silent()).unwrap());
    let st = open();
    assert_eq!(st.license.state(st.now()), LicenseState::Unlicensed);
    activate(&st, &testkit::lifetime(&testkit::device(), now_ms()));
    drop(st);
    let st = open();
    assert!(
        st.license.state(st.now()).allows_writes(),
        "checked again at the start"
    );
    // a backup from before the activation restored: the database copy is gone
    st.db
        .write(|tx| {
            tx.execute(
                "DELETE FROM app_meta WHERE key = ?1",
                [pos_license::store::META_KEY],
            )?;
            Ok(())
        })
        .unwrap();
    drop(st);
    let st = open();
    assert!(
        st.license.state(st.now()).allows_writes(),
        "healed, not tampered"
    );
    let repaired: i64 = st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'license.repaired'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(repaired, 1);
}

#[test]
fn the_seller_contact_is_valid_and_the_qr_names_the_device() {
    let seller = license::seller();
    assert!(
        !seller.seller.is_empty() && seller.url.starts_with("https://"),
        "{seller:?}"
    );
    assert!(
        seller.whatsapp.is_empty()
            || ((9..=15).contains(&seller.whatsapp.len())
                && seller.whatsapp.bytes().all(|b| b.is_ascii_digit())),
        "digits only, international form"
    );
    let code = testkit::device().code().to_owned();
    let with = license::LicenseSeller {
        whatsapp: "213550123456".into(),
        ..seller.clone()
    };
    assert_eq!(
        license::qr_text(&with, &code),
        format!("https://wa.me/213550123456?text=DZBuild%20POS%20{code}")
    );
    let without = license::LicenseSeller {
        whatsapp: String::new(),
        ..seller
    };
    assert_eq!(license::qr_text(&without, &code), code);
}

/// The shell's licence code logs no licence material (pos-license has the same scan for itself).
#[test]
fn no_log_line_of_the_shell_names_licence_material() {
    const LEVELS: [&str; 5] = ["trace!(", "debug!(", "info!(", "warn!(", "error!("];
    const WORDS: [&str; 8] = [
        "key",
        "text",
        "record",
        "seal",
        "secret",
        "payload",
        "signature",
        "verified",
    ];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut calls = 0;
    for file in [
        root.join("license.rs"),
        root.join("commands").join("license.rs"),
    ] {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        // the whole file: its test-only helpers log nothing
        for (at, _) in text.match_indices("tracing::") {
            let rest = &text[at + "tracing::".len()..];
            if !LEVELS.iter().any(|level| rest.starts_with(level)) {
                continue;
            }
            let call = rest[..rest.find(");").unwrap_or(rest.len())].to_lowercase();
            calls += 1;
            for word in WORDS {
                assert!(
                    !call.contains(word),
                    "{}: a log line names `{word}`: {call}",
                    file.display()
                );
            }
        }
    }
    assert!(calls >= 3, "the scan sees the shell's log lines ({calls})");
}

/// Every M8 pack command is classified (Task 21b included): a read-only shop
/// still reads its packs, prints what is written and exports a customer's data, but never takes a
/// deposit, sells a voucher or a top-up, pays an advance, signs a contract, books a round, writes a
/// register, changes a customer's consent or the till's keys, or installs a data pack.
#[test]
fn the_m8_pack_writes_are_never_open_and_their_reads_stay_open() {
    for write in [
        "flexy_purchase",
        "deposits_take",
        "deposits_refund",
        "service_intake",
        "service_pickup",
        "instalments_sign",
        "instalments_collect",
        "rounds_close",
        "rounds_load",
        "orders_create",
        "orders_fulfil",
        "orders_keep_deposit",
        "appointments_book",
        "loyalty_redeem",
        "vouchers_sell",
        "jewellery_reprice",
        "staff_advance",
        "staff_repay",
        "zakat_save",
        "payment_type_save",
        "users_duplicate",
        "privacy_set_consent",
        "privacy_anonymise",
        "till_set_key_map",
        "datapack_install",
        // the restaurant's table tickets and their payment, the floor and the activity,
        // the pharmacy and the butcher's writes; STK-004's lot counts
        "dining_ticket_open",
        "dining_ticket_send",
        "dining_ticket_prebill",
        "dining_ticket_pay",
        "dining_ticket_void_line",
        "packs_apply_template",
        "packs_set_modules",
        "regulated_seed",
        "pharmacy_product_set",
        "chifa_apply",
        "yield_run",
        "waste_record",
        "counts_set_lot",
    ] {
        assert!(
            NEVER_OPEN.contains(&write),
            "{write} must be refused read-only"
        );
    }
    for read in [
        "rounds_list",
        "rounds_route_sheet_print",
        "instalments_contract_print",
        "service_board",
        "orders_list",
        "vouchers_reprint",
        "jeweller_register_entries",
        "zakat_statement",
        "documents_print_receipt_roll",
        "privacy_consents",
        "privacy_export_customer",
        "privacy_register",
        "till_key_map",
        "datapack_installed",
        // the pack state the till reads first, the floor, a bill's preview, the
        // controlled register read, a kitchen ticket printed again; STK-004's lot lines
        "packs_state",
        "dining_board",
        "dining_ticket_preview",
        "controlled_register_entries",
        "kitchen_reprint",
        "sales_points_preview",
        "counts_lot_lines",
    ] {
        assert!(OPEN.contains(&read), "{read} keeps working read-only");
    }
}

#[test]
fn the_free_edition_opens_every_write_with_no_key_and_journals_nothing() {
    let db = pos_core::db::Db::open_memory().unwrap();
    let actor = Actor::system(None);
    let runtime = LicenseRuntime::open(&db, LicenseEnv::for_tests_free(), false, &actor, now_ms());
    let state = runtime.state(now_ms());
    assert_eq!(state, LicenseState::Active(license::free_licence()));
    assert!(state.allows_writes());
    assert!(runtime.check(now_ms()).is_ok());
    // a key pasted or removed changes nothing: the state stays the free lifetime licence
    let key = testkit::lifetime(&testkit::device(), now_ms());
    assert_eq!(
        runtime.activate(&db, &actor, now_ms(), &key).unwrap(),
        LicenseState::Active(license::free_licence())
    );
    assert_eq!(
        runtime.remove(&db, &actor, now_ms()).unwrap(),
        LicenseState::Active(license::free_licence())
    );
    runtime.refresh(&db, &actor, now_ms() + 2 * DAY_MS);
    assert!(runtime.check(now_ms() + 2 * DAY_MS).is_ok());
    let journaled = db
        .read(|c| {
            Ok(c.query_row(
                "SELECT count(*) FROM audit_log WHERE action LIKE 'license.%'",
                [],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap();
    assert_eq!(journaled, 0, "nothing about licences is journaled");
    // this build is the free edition
    assert!(license::free_edition());
}
