//! "Task 21b": the rows M6 handed over, wired on core Task 17 — the
//! business-day cut-off at every till, signed data packs installed for the next start and loaded
//! at start, personal data (consents, access journal, export, anonymisation, the processing
//! register), the till's key map, the hourly backup and the background quick check.

use super::testshop::{TestShop, shop};
use super::{customers as customers_cmd, privacy, settings};
use crate::lan::vault::MemoryVault;
use crate::lan::{LanOptions, rpc::SERVICES};
use crate::state::{AppPaths, AppState, StartupNotice};
use pos_core::auth::Permission;
use pos_core::customers::CustomerInput;
use pos_core::lang::Lang;
use pos_core::privacy::{ConsentChannel, ConsentSource};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/pos-core/tests/fixtures/datapack")
}

/// The public key of the test-only signer (`test-only.key.pub`): never trusted by the app.
fn test_key() -> String {
    std::fs::read_to_string(fixtures().join("test-only.key.pub")).unwrap()
}

fn audit_rows(t: &TestShop, action: &'static str) -> i64 {
    t.st.db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE action = ?1",
                [action],
                |r| r.get(0),
            )?)
        })
        .unwrap()
}

/// Karima with a phone (personal data), or Passant without any.
fn customer(t: &TestShop, name: &str, phone: Option<&str>) -> String {
    let (actor, pack) = (t.owner.actor(), t.st.datapack.clone());
    let input = CustomerInput {
        phone: phone.map(str::to_owned),
        ..CustomerInput::named(name)
    };
    t.st.db
        .write(move |tx| pos_core::customers::create(tx, &actor, &pack, input))
        .unwrap()
        .id
}

#[test]
fn every_till_context_follows_the_business_day_cutoff() {
    let t = shop();
    let st = &t.st;
    assert_eq!(
        st.till(Permission::SalesCreate).unwrap().day_cutoff_hours,
        0
    );
    settings::set(st, "sales.day_cutoff_hour".into(), serde_json::json!(4)).unwrap();
    let ctx = st.till(Permission::SalesCreate).unwrap();
    assert_eq!(ctx.day_cutoff_hours, 4, "CFG-008 at every till write");
}

#[test]
fn a_signed_data_pack_is_installed_for_the_next_start() {
    let t = shop();
    let st = &t.st;
    let dir = tempfile::tempdir().unwrap();
    let (json, sig) = (
        fixtures().join("dz-datapack-test-v2.json"),
        fixtures().join("dz-datapack-test-v2.json.sig"),
    );
    let refused = privacy::install_files(
        st,
        dir.path(),
        &json,
        &sig,
        pos_core::datapack_update::DATAPACK_PUBLIC_KEY,
    )
    .unwrap_err();
    assert_eq!(
        refused.code(),
        "datapack_signature_invalid",
        "the app trusts the owner's key, never the test key"
    );
    assert!(
        privacy::installed_in(st, dir.path(), &test_key())
            .unwrap()
            .is_empty(),
        "nothing written on a refusal"
    );
    assert_eq!(audit_rows(&t, "datapack.installed"), 0);

    // signed with the key it is checked against: installed for the next start and journaled
    let done = privacy::install_files(st, dir.path(), &json, &sig, &test_key()).unwrap();
    assert!(done.version > st.datapack.version);
    let listed = privacy::installed_in(st, dir.path(), &test_key()).unwrap();
    assert_eq!(listed, vec![done.clone()]);
    assert_eq!(audit_rows(&t, "datapack.installed"), 1);
    let again = privacy::install_files(st, dir.path(), &json, &sig, &test_key());
    assert!(
        again.is_ok(),
        "the same newer pack installs again over itself"
    );

    // a cashier never installs one
    t.cashier();
    let refused = privacy::install_files(st, dir.path(), &json, &sig, &test_key()).unwrap_err();
    assert_eq!(refused.code(), "insufficient_permission");
}

/// PLT-024 at start: a file of `<data>\datapacks` that fails its signature is named in a notice
/// and never used; the bundled pack stays in force.
#[test]
fn the_start_names_a_data_pack_it_left_aside() {
    let dir = tempfile::tempdir().unwrap();
    let packs = dir.path().join("datapacks");
    std::fs::create_dir_all(&packs).unwrap();
    std::fs::copy(
        fixtures().join("dz-datapack-test-v2.json"),
        packs.join("dz-datapack-test-v2.json"),
    )
    .unwrap();
    std::fs::copy(
        fixtures().join("dz-datapack-test-v2.json.sig"),
        packs.join("dz-datapack-test-v2.json.sig"),
    )
    .unwrap();
    std::fs::write(packs.join("broken.json"), "{}").unwrap();
    let st = AppState::open_with(
        &AppPaths::new(dir.path()),
        crate::printing::silent(),
        Arc::new(MemoryVault::default()),
        LanOptions {
            bind: std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
            advertise: false,
        },
    )
    .unwrap();
    let skipped: Vec<(String, String)> = st
        .notices
        .iter()
        .filter_map(|n| match n {
            StartupNotice::DatapackSkipped { file, code } => Some((file.clone(), code.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        skipped,
        vec![
            ("broken.json".into(), "datapack_signature_invalid".into()),
            (
                "dz-datapack-test-v2.json".into(),
                "datapack_signature_invalid".into()
            ),
        ]
    );
    assert_eq!(
        st.datapack.version,
        pos_core::datapack::bundled().unwrap().version,
        "the bundled pack stays in force"
    );
}

#[test]
fn opening_a_customer_file_is_journaled_and_the_register_page_builds() {
    let t = shop();
    let st = &t.st;
    let karima = customer(&t, "Karima", Some("0550123456"));
    let passant = customer(&t, "Passant", None);
    customers_cmd::get(st, karima.clone()).unwrap();
    customers_cmd::get(st, passant).unwrap();
    assert_eq!(
        audit_rows(&t, "privacy.accessed"),
        1,
        "only a file holding personal data"
    );
    let payload: String = st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT payload FROM audit_log WHERE action = 'privacy.accessed'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert!(payload.contains("\"view\"") && !payload.contains("0550123456"));
    let page = privacy::register(st, Lang::Fr).unwrap();
    let crate::print_host::HostPage::ProcessingRegister(print) = page else {
        panic!("the processing register page");
    };
    assert_eq!(print.lang, Lang::Fr);
    assert!(!print.activities.is_empty(), "the customer file at least");
    t.cashier();
    assert_eq!(
        privacy::register(st, Lang::Fr).unwrap_err().code(),
        "insufficient_permission"
    );
}

#[test]
fn consents_export_and_anonymisation_run_through_their_commands() {
    let t = shop();
    let st = &t.st;
    let karima = customer(&t, "Karima", Some("0550123456"));
    privacy::set_consent(
        st,
        karima.clone(),
        ConsentChannel::Sms,
        true,
        ConsentSource::Counter,
        None,
    )
    .unwrap();
    privacy::set_consent(
        st,
        karima.clone(),
        ConsentChannel::Sms,
        false,
        ConsentSource::CustomerRequest,
        Some("au téléphone".into()),
    )
    .unwrap();
    let history = privacy::consents(st, karima.clone()).unwrap();
    assert_eq!(
        history.iter().map(|c| c.granted).collect::<Vec<_>>(),
        [false, true],
        "every answer, newest first"
    );

    let dir = tempfile::tempdir().unwrap();
    let files = privacy::export_customer(st, karima.clone(), dir.path()).unwrap();
    let json = std::fs::read_to_string(&files.json_path).unwrap();
    assert!(json.contains("0550123456") && json.contains("\"consents\""));
    assert!(Path::new(&files.xlsx_path).exists());
    assert_eq!(audit_rows(&t, "privacy.exported"), 1);

    let report = privacy::anonymise(st, karima.clone(), Some("demande écrite".into())).unwrap();
    assert_eq!(report.customer_id, karima);
    let id = karima.clone();
    let erased = st
        .db
        .read(move |c| pos_core::customers::get(c, &id))
        .unwrap();
    assert_eq!(erased.phone, None);
    assert_eq!(audit_rows(&t, "privacy.anonymised"), 1);

    t.cashier();
    assert_eq!(
        privacy::anonymise(st, karima, None).unwrap_err().code(),
        "insufficient_permission"
    );
}

#[test]
fn the_till_key_map_is_read_by_everyone_and_set_by_the_owner() {
    let t = shop();
    let st = &t.st;
    let defaults = privacy::key_map(st).unwrap();
    assert_eq!(defaults.len(), pos_core::till::DEFAULT_KEYS.len());
    let map = [("drawer".to_owned(), "Ctrl+9".to_owned())]
        .into_iter()
        .collect();
    let rows = privacy::set_key_map(st, map).unwrap();
    assert!(rows.contains(&("drawer".to_owned(), "Ctrl+9".to_owned())));
    let taken = [("drawer".to_owned(), "F1".to_owned())]
        .into_iter()
        .collect();
    assert_eq!(
        privacy::set_key_map(st, taken).unwrap_err().code(),
        "key_map_invalid",
        "F1 is help's"
    );
    t.cashier();
    assert!(
        privacy::key_map(st)
            .unwrap()
            .contains(&("drawer".to_owned(), "Ctrl+9".to_owned()))
    );
}

/// A till asks the main PC for the customer books' writes.
#[test]
fn consents_and_anonymisation_are_host_services() {
    assert!(SERVICES.contains(&"privacy.set_consent"));
    assert!(SERVICES.contains(&"privacy.anonymise"));
}

/// DAT-007: an hourly copy only while this PC's drawer is open (a shop that is not selling has
/// nothing new to lose); DAT-011: a sound database passes the background check.
#[test]
fn the_hourly_backup_runs_while_the_drawer_is_open_and_the_quick_check_passes() {
    let root = tempfile::tempdir().unwrap();
    let st = AppState::open_with(
        &AppPaths::new(root.path()),
        crate::printing::silent(),
        Arc::new(MemoryVault::default()),
        LanOptions {
            bind: std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
            advertise: false,
        },
    )
    .unwrap();
    let t = super::testshop::first_run_shop(Arc::new(st));
    let autos = |t: &TestShop| {
        t.st.db
            .list_backups()
            .unwrap()
            .into_iter()
            .filter(|b| b.kind == pos_core::db::BackupKind::Auto)
            .count()
    };
    let before = autos(&t);
    crate::state::run_hourly_backup(&t.st);
    assert_eq!(autos(&t), before, "no drawer open: no hourly copy");
    t.open_till(10_000);
    crate::state::run_hourly_backup(&t.st);
    assert_eq!(
        autos(&t),
        before + 1,
        "an hourly copy once the drawer is open"
    );
    crate::state::run_hourly_backup(&t.st);
    assert_eq!(autos(&t), before + 1, "one an hour");
    crate::state::run_quick_check(&t.st);
    assert!(
        !t.st
            .backup_status(pos_core::time::now_ms())
            .database_damaged
    );
}
