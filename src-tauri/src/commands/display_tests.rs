use super::display::{self as commands, DisplayPrefs};
use super::tests::first_run;
use crate::state::{AppPaths, AppState, SharedState};
use pos_core::local::{LocalConfig, TextSize};
use std::path::Path;
use std::sync::Arc;

/// Light mode, the text size and the on-screen keyboard are this PC's (`local.json`), one
/// `local.updated` per changed choice; the login screen and the wizard read the file itself.
#[test]
fn display_choices_are_saved_for_this_pc_and_journaled() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    let st: SharedState = Arc::new(AppState::open(&paths, crate::printing::silent()).unwrap());
    first_run(&st);
    assert_eq!(
        commands::get(Some(st.as_ref()), &paths),
        DisplayPrefs::from(&LocalConfig::default())
    );
    let prefs = DisplayPrefs {
        light_mode: true,
        text_size: TextSize::Large,
        onscreen_keyboard: false,
    };
    assert_eq!(commands::set(&st, prefs).unwrap(), prefs);
    assert_eq!(
        DisplayPrefs::from(&LocalConfig::load(Path::new(&paths.local_config))),
        prefs
    );
    assert_eq!(commands::get(Some(st.as_ref()), &paths), prefs);
    assert_eq!(
        commands::get(None, &paths),
        prefs,
        "before the database opens"
    );
    let keys: Vec<String> = st
        .db
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT entity_id FROM audit_log WHERE action = 'local.updated' ORDER BY seq",
            )?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            Ok(rows.collect::<Result<Vec<String>, _>>()?)
        })
        .unwrap();
    assert_eq!(keys, ["lightMode", "textSize"]);
}

#[test]
fn display_choices_need_settings_hardware() {
    let st = AppState::for_tests();
    let prefs = DisplayPrefs::from(&LocalConfig::default());
    assert_eq!(
        commands::set(&st, prefs).unwrap_err().code(),
        "session_required"
    );
}
