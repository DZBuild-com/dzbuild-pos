//! A setup Windows does not start (the owner says No to the administrator prompt, or this
//! account cannot approve it): once the before-exit hook closed the database, hid the windows
//! and removed the tray, the app restarts with a notice; before the hook, it keeps running.

use super::tests::first_run;
use super::updates_tests::manifest_for;
use crate::state::{AppPaths, AppState, SharedState, StartupNotice};
use crate::updates::{NOT_INSTALLED_MARKER, RemoteUpdate, UpdateService, UpdateSource};
use pos_core::db::Db;
use pos_core::error::AppError;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const ENDPOINT: &str = "https://dzbuild.com/pos/updates/stable/latest.json";

/// A source whose setup never starts: `hook_runs` says whether the before-exit hook ran first
/// (the plugin runs it before `ShellExecuteW`) or the setup failed before it (a bad archive).
struct Refused {
    manifest: serde_json::Value,
    bytes: Vec<u8>,
    hook_runs: bool,
    db: Db,
    shut_down: AtomicBool,
    restarts: AtomicUsize,
}

impl UpdateSource for Refused {
    fn check<'a>(
        &'a self,
        _endpoint: &'a str,
    ) -> crate::updates::BoxFuture<'a, Option<RemoteUpdate>> {
        Box::pin(async move {
            Ok(Some(RemoteUpdate {
                version: "9.9.9".into(),
                raw: self.manifest.clone(),
                handle: None,
                mirror: false,
            }))
        })
    }

    fn download<'a>(
        &'a self,
        _update: &'a RemoteUpdate,
        _progress: Box<dyn FnMut(u64, Option<u64>) + Send + 'a>,
    ) -> crate::updates::BoxFuture<'a, Vec<u8>> {
        Box::pin(async move { Ok(self.bytes.clone()) })
    }

    fn install(&self, _update: &RemoteUpdate, _bytes: Vec<u8>) -> Result<(), AppError> {
        if self.hook_runs {
            self.shut_down.store(true, Ordering::SeqCst);
            self.db.mark_clean_shutdown().unwrap();
        }
        Err(AppError::Internal(
            "update install: The operation was canceled by the user. (os error 1223)".into(),
        ))
    }

    fn has_shut_down(&self) -> bool {
        self.shut_down.load(Ordering::SeqCst)
    }

    fn restart(&self) {
        self.restarts.fetch_add(1, Ordering::SeqCst);
    }
}

fn ready_to_install(dir: &Path, hook_runs: bool) -> (SharedState, Arc<Refused>, UpdateService) {
    let st: SharedState =
        Arc::new(AppState::open(&AppPaths::new(dir), crate::printing::silent()).unwrap());
    first_run(&st);
    let bytes = b"the signed setup".to_vec();
    let source = Arc::new(Refused {
        manifest: manifest_for(&bytes, json!({})),
        bytes,
        hook_runs,
        db: st.db.clone(),
        shut_down: AtomicBool::new(false),
        restarts: AtomicUsize::new(0),
    });
    let service = UpdateService::new(source.clone(), ENDPOINT.into());
    tauri::async_runtime::block_on(service.check_now(&st, true)).unwrap();
    tauri::async_runtime::block_on(service.download(&st, |_, _| {})).unwrap();
    (st, source, service)
}

fn not_installed(notices: &[StartupNotice]) -> Vec<String> {
    notices
        .iter()
        .filter_map(|notice| match notice {
            StartupNotice::UpdateNotInstalled { version } => Some(version.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_setup_that_does_not_start_after_the_database_closed_restarts_the_app_with_a_notice() {
    let dir = tempfile::tempdir().unwrap();
    let (st, source, service) = ready_to_install(dir.path(), true);
    let session = st.session().unwrap();
    let error = service.install(&st, session.actor()).unwrap_err();
    assert_eq!(error.code(), "internal_error");
    assert!(!service.status(&st).unwrap().installing);
    assert_eq!(
        source.restarts.load(Ordering::SeqCst),
        1,
        "a process whose database is closed restarts"
    );
    assert!(
        dir.path().join(NOT_INSTALLED_MARKER).is_file(),
        "the next start explains why"
    );
    drop((service, source, st));

    let next = AppState::open(&AppPaths::new(dir.path()), crate::printing::silent()).unwrap();
    assert_eq!(not_installed(&next.notices), ["9.9.9"]);
    next.db
        .write(|tx| pos_core::db::meta_set(tx, "after", "restart"))
        .expect("the restarted app has a working writer");
    assert!(!dir.path().join(NOT_INSTALLED_MARKER).exists());
    drop(next);

    let again = AppState::open(&AppPaths::new(dir.path()), crate::printing::silent()).unwrap();
    assert!(
        not_installed(&again.notices).is_empty(),
        "the notice shows once"
    );
}

#[test]
fn a_setup_that_fails_before_the_database_closes_keeps_the_app_running() {
    let dir = tempfile::tempdir().unwrap();
    let (st, source, service) = ready_to_install(dir.path(), false);
    let session = st.session().unwrap();
    assert_eq!(
        service.install(&st, session.actor()).unwrap_err().code(),
        "internal_error"
    );
    assert!(!service.status(&st).unwrap().installing);
    assert_eq!(source.restarts.load(Ordering::SeqCst), 0);
    assert!(!dir.path().join(NOT_INSTALLED_MARKER).exists());
    st.db
        .write(|tx| pos_core::db::meta_set(tx, "after", "failed install"))
        .expect("the database still takes writes");
}
