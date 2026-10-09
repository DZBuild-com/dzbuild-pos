use super::demo;
use super::tests::first_run;
use crate::state::{AppPaths, AppState};
use pos_core::lang::Lang;
use std::path::Path;

#[test]
fn entering_the_demo_seeds_it_once_and_leaves_the_real_shop_alone() {
    let dir = tempfile::tempdir().unwrap();
    let real = AppPaths::for_profile(dir.path(), false);
    let now = pos_core::time::now_ms();
    let summary = demo::enter_now(&real, Lang::Ar, now).unwrap();
    assert_eq!(summary.tickets, 48);
    assert!(pos_core::demo::is_active(dir.path()));
    let demo_paths = AppPaths::for_profile(dir.path(), true);
    assert!(Path::new(&demo_paths.db_path).exists());
    let again = demo::enter_now(&real, Lang::Ar, now + 1_000).unwrap();
    assert_eq!(again.tickets, 48, "the demo is seeded once");
    let state = AppState::open(&demo_paths, crate::printing::silent()).unwrap();
    let lang: String = state
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT value FROM settings WHERE key = 'ui.login_lang'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(
        lang, "\"ar\"",
        "the demo speaks the language chosen in the wizard"
    );
    assert!(
        !Path::new(&real.db_path).exists(),
        "the real shop was never created or opened"
    );
}

#[test]
fn only_a_new_install_or_an_owner_may_enter_the_demo_or_import_a_shop() {
    demo::may_enter(None).unwrap(); // the wizard before any database opened
    let st = AppState::for_tests();
    demo::may_enter(Some(&*st)).unwrap(); // setup not done yet
    demo::may_import(Some(&*st)).unwrap();
    first_run(&st);
    demo::may_enter(Some(&*st)).unwrap(); // the owner is signed in
    assert_eq!(
        demo::may_import(Some(&*st)).unwrap_err().code(),
        "setup_already_done"
    );
    crate::commands::auth::logout(&st).unwrap();
    assert_eq!(
        demo::may_enter(Some(&*st)).unwrap_err().code(),
        "session_required"
    );
}

/// The real shop and its demo in one data folder: the demo seeded, opened on the demo profile,
/// its owner signed in, the vault holding the real till's token.
fn demo_profile(
    dir: &Path,
) -> (
    std::sync::Arc<AppState>,
    std::sync::Arc<crate::lan::vault::MemoryVault>,
) {
    use crate::lan::vault::TokenVault;
    let real = AppPaths::for_profile(dir, false);
    let summary = demo::enter_now(&real, Lang::Fr, pos_core::time::now_ms()).unwrap();
    let vault = std::sync::Arc::new(crate::lan::vault::MemoryVault::default());
    vault.set("the-real-tills-token").unwrap();
    let st = std::sync::Arc::new(
        AppState::open_with(
            &AppPaths::for_profile(dir, true),
            crate::printing::silent(),
            vault.clone(),
            crate::lan::LanOptions {
                bind: std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
                advertise: false,
            },
        )
        .unwrap(),
    );
    crate::commands::auth::login(
        &st,
        &crate::commands::auth::LoginInput {
            user: summary.owner_id,
            pin: pos_core::demo::OWNER_PIN.into(),
        },
    )
    .unwrap();
    (st, vault)
}

/// Inside the demo shop, nothing joins a network, becomes a main PC, links DZBuild or
/// chooses a second copy: one code, before any file, vault or network is touched.
#[test]
fn the_demo_shop_refuses_the_network_dzbuild_and_the_real_backup_media() {
    use crate::lan::vault::TokenVault;
    let dir = tempfile::tempdir().unwrap();
    let (st, vault) = demo_profile(dir.path());
    let sink = super::lan::quiet();
    let code = |result: Result<(), pos_core::error::AppError>| result.unwrap_err().code();
    let join = super::lan::JoinInput {
        address: "127.0.0.1:9".into(),
        code: "042917".into(),
        terminal_code: "C2".into(),
        terminal_name: "Caisse C2".into(),
        owner_pin: Some(pos_core::demo::OWNER_PIN.into()),
    };
    let refused = [
        code(super::lan::may_join(&st)), // lan_discover, lan_probe, lan_restart
        code(
            super::lan::join(&st, join, std::sync::Arc::new(|_: u64, _: Option<u64>| {})).map(drop),
        ),
        code(super::lan::become_host(&st, &sink).map(drop)),
        code(super::lan::pairing_code(&st).map(drop)),
        code(super::lan::promote(&st, pos_core::demo::OWNER_PIN)),
        code(super::lan::leave(&st, pos_core::demo::OWNER_PIN)),
        code(super::cloud::link_start(&st, Lang::Fr).map(drop)),
        code(super::cloud::device_flow_start(&st).map(drop)),
        code(super::backup::mirror_set(&st, dir.path().join("stick")).map(drop)),
        code(super::backup::mirror_clear(&st).map(drop)),
        code(super::backup::to_drive(&st, "E:\\").map(drop)),
    ];
    assert!(
        refused.iter().all(|c| c == "demo_not_allowed"),
        "{refused:?}"
    );
    assert!(
        !dir.path().join("lan").exists(),
        "nothing staged for the real shop"
    );
    assert_eq!(
        vault.get().unwrap().as_deref(),
        Some("the-real-tills-token"),
        "the real till's token is untouched"
    );
    assert!(!dir.path().join("stick").exists());
    assert_eq!(st.lan.role(), pos_core::terminal::TerminalRole::Standalone);
}

/// Finding 39: the demo profile's database is marked, its backups never reach the real shop's
/// second copy, a demo backup never replaces the real shop (nor a real one the demo), and
/// leaving the demo is journaled in the real `pos.db` at its next start.
#[test]
fn the_demo_and_the_real_shop_never_take_each_others_place() {
    let dir = tempfile::tempdir().unwrap();
    let (st, _) = demo_profile(dir.path());
    assert!(st.db.read(pos_core::demo::is_marked).unwrap());
    let company = st.db.read(pos_core::company::get).unwrap().unwrap();
    assert!(company.demo);
    // The real shop's second copy, chosen before the demo started.
    let stick = dir.path().join("..").join(format!(
        "stick-{}",
        dir.path().file_name().unwrap().to_string_lossy()
    ));
    std::fs::create_dir_all(&stick).unwrap();
    st.local
        .update(&st.db, pos_core::audit::Actor::system(None), |c| {
            c.backup_mirror = Some(pos_core::local::MirrorFolder {
                path: stick.to_string_lossy().into_owned(),
                label: None,
            })
        })
        .unwrap();
    let demo_copy = super::backup::backup_now_impl(&st).unwrap();
    assert!(
        std::fs::read_dir(&stick).unwrap().next().is_none(),
        "the demo copy stays in its own backups folder"
    );
    assert_eq!(super::backup::mirror_status(&st).unwrap().folder, None);
    assert_eq!(
        pos_core::db::recovery::restore_guard(Path::new(&demo_copy.path), false)
            .unwrap_err()
            .code(),
        "backup_is_demo"
    );
    pos_core::db::recovery::restore_guard(Path::new(&demo_copy.path), true).unwrap();
    let preview = pos_core::db::recovery::inspect_backup(Path::new(&demo_copy.path));
    assert!(preview.origin.unwrap().demo, "the preview says DÉMO");
    // Its `.dzpos` file says so too (finding 32).
    let exported = dir.path().join("demo-export.dzpos");
    super::data_files::export_backup_to(&st, &exported).unwrap();
    assert!(pos_docs::archive::read_manifest(&exported).unwrap().demo);
    // Leaving: noted for the real shop's journal, then the real shop's start writes the row.
    demo::leave_now(Some(&*st), dir.path(), 1_790_593_200_000).unwrap();
    drop(st);
    let real = AppState::open(
        &AppPaths::for_profile(dir.path(), false),
        crate::printing::silent(),
    )
    .unwrap();
    let left: i64 = real
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'demo.left'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(left, 1);
    assert!(!dir.path().join(pos_core::demo::LEFT_NOTE).exists());
    assert!(!real.db.read(pos_core::demo::is_marked).unwrap());
    let real_copy = real.db.backup(pos_core::db::BackupKind::Manual).unwrap();
    assert_eq!(
        pos_core::db::recovery::restore_guard(Path::new(&real_copy.path), true)
            .unwrap_err()
            .code(),
        "demo_not_allowed"
    );
    let _ = std::fs::remove_dir_all(&stick);
}
