use super::diagnostics as commands;
use super::tests::first_run;
use crate::diagnostics::{crash_dump_count, export_to, last_crash};
use crate::state::{AppPaths, AppState, SharedState};
use pos_core::audit::Actor;
use pos_core::customers::{self, CustomerInput};
use pos_core::local::LogLevel;
use std::io::Read;
use std::sync::Arc;

#[test]
fn the_exported_zip_names_its_sections_and_holds_no_customer() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    let st: SharedState = Arc::new(AppState::open(&paths, crate::printing::silent()).unwrap());
    first_run(&st);
    let pack = st.datapack.clone();
    st.db
        .write(move |tx| {
            let mut input = CustomerInput::named("Karim Benali");
            input.phone = Some("0550123456".into());
            customers::create(tx, &Actor::system(None), &pack, input).map(drop)
        })
        .unwrap();
    std::fs::create_dir_all(&paths.logs_dir).unwrap();
    std::fs::write(
        std::path::Path::new(&paths.logs_dir).join("dzbuild-pos.2026-09-28.log"),
        "INFO sale for Karim Benali 0550123456\n",
    )
    .unwrap();
    let out = dir.path().join("diag.zip");
    let summary = export_to(&st, &paths, serde_json::json!({ "os": "Windows 10" }), &out).unwrap();
    assert!(summary.entries.contains(&"snapshot.json".to_owned()));
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        assert!(
            !text.contains("Karim") && !text.contains("0550123456"),
            "{}",
            entry.name()
        );
    }
    let journaled: i64 = st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'diagnostics.exported'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(journaled, 1);
}

#[test]
fn the_last_crash_is_read_from_crash_log_and_masked() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(last_crash(dir.path()), None);
    std::fs::write(
        dir.path().join("crash.log"),
        "[2026-09-27 10:00:00.000 Algiers] panic in thread 'main': first\n\
         [2026-09-28 12:00:00.000 Algiers] panic in thread 'pos-db-writer': customer 0550123456 at src/x.rs:1:1\n",
    )
    .unwrap();
    let crash = last_crash(dir.path()).unwrap();
    assert_eq!(crash.at, "2026-09-28 12:00:00.000 Algiers");
    assert!(
        crash.message.contains("pos-db-writer") && crash.message.contains("<phone>"),
        "{}",
        crash.message
    );
    assert_eq!(crash_dump_count(&dir.path().join("none")), 0);
    std::fs::create_dir_all(dir.path().join("dumps")).unwrap();
    std::fs::write(
        dir.path().join("dumps").join("dzbuild-pos.exe.4120.dmp"),
        b"MDMP",
    )
    .unwrap();
    assert_eq!(crash_dump_count(&dir.path().join("dumps")), 1);
}

#[test]
fn support_links_come_from_the_setting_only() {
    use commands::{SupportChannel, SupportContact, support_url};
    let contact = SupportContact {
        whatsapp: Some("213550123456".into()),
        url: Some("https://dzbuild.com/pos/support".into()),
        label: None,
    };
    assert_eq!(
        support_url(&contact, SupportChannel::Whatsapp).unwrap(),
        "https://wa.me/213550123456"
    );
    assert_eq!(
        support_url(&contact, SupportChannel::Web).unwrap(),
        "https://dzbuild.com/pos/support"
    );
    let empty = SupportContact {
        whatsapp: None,
        url: None,
        label: None,
    };
    assert_eq!(
        support_url(&empty, SupportChannel::Whatsapp)
            .unwrap_err()
            .code(),
        "support_contact_missing"
    );
}

#[test]
fn the_log_level_is_saved_and_journaled() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    let st: SharedState = Arc::new(AppState::open(&paths, crate::printing::silent()).unwrap());
    first_run(&st);
    assert_eq!(
        commands::set_log_level(&st, LogLevel::Debug).unwrap(),
        LogLevel::Debug
    );
    assert_eq!(
        pos_core::local::LocalConfig::load(std::path::Path::new(&paths.local_config)).log_level,
        LogLevel::Debug
    );
    let rows: i64 = st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'local.updated'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(rows, 1);
}

/// Security rule (binding): whatever the logs, `local.json` or `crash.log` hold, the zip carries
/// no PIN hash, token, pairing code or database, only what `sanitize` lets through.
#[test]
fn the_zip_carries_no_secret_and_no_database() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    let st: SharedState = Arc::new(AppState::open(&paths, crate::printing::silent()).unwrap());
    first_run(&st);
    let pin_hash: String = st
        .db
        .read(|c| Ok(c.query_row("SELECT pin_hash FROM users LIMIT 1", [], |r| r.get(0))?))
        .unwrap();
    let lan_token = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
    let logs = std::path::Path::new(&paths.logs_dir);
    std::fs::create_dir_all(logs).unwrap();
    std::fs::write(
        logs.join("dzbuild-pos.2026-09-29.log"),
        format!(
            "INFO sign-in pin=2580 hash {pin_hash}\nINFO joining code=482913\n\
             WARN Authorization: Bearer at_SECRET_ACCESS\nINFO refresh_token=rt_SECRET\n"
        ),
    )
    .unwrap();
    crate::logging::write_crash_record(logs, "panicked at src/x.rs:1:1:\nkarim@example.dz")
        .unwrap();
    std::fs::write(
        &paths.local_config,
        format!(
            r#"{{ "logLevel": "debug", "lan": {{ "role": "till", "token": "{lan_token}" }} }}"#
        ),
    )
    .unwrap();
    let out = dir.path().join("diag.zip");
    let summary = export_to(&st, &paths, serde_json::json!({}), &out).unwrap();
    assert!(
        summary.entries.contains(&"local.json".to_owned())
            && summary.entries.contains(&"logs/crash.log".to_owned()),
        "{:?}",
        summary.entries
    );
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().to_owned();
        assert!(!name.ends_with(".db") && !name.ends_with(".dmp"), "{name}");
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.starts_with("SQLite format 3"), "{name}");
        for leaked in [
            pin_hash.as_str(),
            "argon2",
            "2580",
            "482913",
            "at_SECRET_ACCESS",
            "rt_SECRET",
            lan_token,
            "karim@example.dz",
        ] {
            assert!(!text.contains(leaked), "{leaked} in {name}: {text}");
        }
    }
}

/// A panic's message comes on the lines after its record's first line (`PanicHookInfo` prints
/// the place, then the message): the last crash shows both.
#[test]
fn the_last_crash_keeps_the_panic_message_of_its_record() {
    let dir = tempfile::tempdir().unwrap();
    crate::logging::write_crash_record(dir.path(), "panicked at src/a.rs:1:1:\nan older one")
        .unwrap();
    crate::logging::write_crash_record(
        dir.path(),
        "panicked at src/state.rs:42:7:\nindex out of bounds: the len is 0 but the index is 3",
    )
    .unwrap();
    let crash = last_crash(dir.path()).unwrap();
    assert!(crash.at.ends_with(" Algiers"), "{}", crash.at);
    assert!(
        crash.message.contains("src/state.rs:42:7")
            && crash.message.contains("index out of bounds")
            && !crash.message.contains("older"),
        "{}",
        crash.message
    );
}
