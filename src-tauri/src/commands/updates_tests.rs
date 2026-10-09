//! The updater's policy over a fake source: nothing here touches the network or installs.

use super::tests::first_run;
use crate::state::{AppPaths, AppState, SharedState};
use crate::updates::{DownloadState, RemoteUpdate, UpdateService, UpdateSource, map_plugin_error};
use pos_core::audit::Actor;
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::money::Money;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

const ENDPOINT: &str = "https://dzbuild.com/pos/updates/stable/latest.json";

struct Fake {
    /// What the channel answers now (a release can be paused after its download).
    manifest: Mutex<Option<serde_json::Value>>,
    /// The channel does not answer (no Internet).
    offline: std::sync::atomic::AtomicBool,
    bytes: Vec<u8>,
    /// The download never ends and never says anything (a stalled transfer).
    stall: std::sync::atomic::AtomicBool,
    installed: Mutex<Vec<String>>,
    before_exit: Box<dyn Fn() + Send + Sync>,
}

impl UpdateSource for Fake {
    fn check<'a>(
        &'a self,
        endpoint: &'a str,
    ) -> crate::updates::BoxFuture<'a, Option<RemoteUpdate>> {
        Box::pin(async move {
            assert_eq!(endpoint, ENDPOINT);
            if self.offline.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(AppError::Sync {
                    code: "update_check_failed".into(),
                });
            }
            Ok(self
                .manifest
                .lock()
                .unwrap()
                .clone()
                .map(|manifest| RemoteUpdate {
                    version: manifest["version"].as_str().unwrap_or_default().to_owned(),
                    raw: manifest,
                    handle: None,
                    mirror: false,
                }))
        })
    }

    fn download<'a>(
        &'a self,
        _update: &'a RemoteUpdate,
        mut progress: Box<dyn FnMut(u64, Option<u64>) + Send + 'a>,
    ) -> crate::updates::BoxFuture<'a, Vec<u8>> {
        Box::pin(async move {
            if self.stall.load(std::sync::atomic::Ordering::SeqCst) {
                std::future::pending::<()>().await;
            }
            progress(self.bytes.len() as u64 / 2, Some(self.bytes.len() as u64));
            progress(self.bytes.len() as u64, Some(self.bytes.len() as u64));
            Ok(self.bytes.clone())
        })
    }

    fn install(&self, update: &RemoteUpdate, _bytes: Vec<u8>) -> Result<(), AppError> {
        (self.before_exit)();
        self.installed.lock().unwrap().push(update.version.clone());
        Ok(())
    }

    fn has_shut_down(&self) -> bool {
        !self.installed.lock().unwrap().is_empty()
    }

    fn restart(&self) {
        panic!("a setup that started never restarts the app");
    }
}

pub(super) fn manifest_for(bytes: &[u8], extra: serde_json::Value) -> serde_json::Value {
    let mut m = json!({
        "version": "9.9.9",
        "notes": "Corrections",
        "notes_i18n": { "ar": "تصحيحات", "fr": "Corrections", "en": "Fixes" },
        "platforms": { "windows-x86_64-nsis": {
            "url": "https://dzbuild.com/pos/download/9.9.9/DZBuildPOS-9.9.9-setup.exe",
            "signature": "sig",
            "sha256": hex::encode(Sha256::digest(bytes)),
            "size": bytes.len()
        } }
    });
    if let (Some(map), Some(extra)) = (m.as_object_mut(), extra.as_object()) {
        map.extend(extra.clone());
    }
    m
}

fn setup(
    bytes_served: Vec<u8>,
    manifest: serde_json::Value,
) -> (tempfile::TempDir, SharedState, Arc<Fake>, UpdateService) {
    let dir = tempfile::tempdir().unwrap();
    let st: SharedState =
        Arc::new(AppState::open(&AppPaths::new(dir.path()), crate::printing::silent()).unwrap());
    first_run(&st);
    let closing = st.clone();
    let fake = Arc::new(Fake {
        manifest: Mutex::new(Some(manifest)),
        offline: std::sync::atomic::AtomicBool::new(false),
        bytes: bytes_served,
        stall: std::sync::atomic::AtomicBool::new(false),
        installed: Mutex::new(Vec::new()),
        before_exit: Box::new(move || {
            closing.db.mark_clean_shutdown().unwrap();
        }),
    });
    let service = UpdateService::new(fake.clone(), ENDPOINT.into());
    (dir, st, fake, service)
}

fn open_drawer(st: &AppState) {
    let ctx = st.till(Permission::CashOpen).unwrap();
    st.db
        .write(move |tx| pos_core::cash::open_session(tx, &ctx, Money::ZERO).map(drop))
        .unwrap();
}

fn close_drawer(st: &AppState) {
    super::cash::close_session(st, pos_core::cash::CloseInput::default()).unwrap();
}

fn count(st: &AppState, sql: &'static str) -> i64 {
    st.db
        .read(move |c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .unwrap()
}

#[test]
fn a_check_offers_only_what_the_policy_allows() {
    let bytes = b"setup".to_vec();
    let (_d, st, _f, service) = setup(
        bytes.clone(),
        manifest_for(&bytes, json!({ "paused": true })),
    );
    let status = tauri::async_runtime::block_on(service.check_now(&st, true)).unwrap();
    let offer = status.offer.unwrap();
    assert_eq!(offer.decision, pos_core::updates::Decision::Paused);
    assert_eq!(offer.notes.unwrap().ar, "تصحيحات");
    assert_eq!(
        tauri::async_runtime::block_on(service.download(&st, |_, _| {}))
            .unwrap_err()
            .code(),
        "update_not_available",
        "a paused release is never downloaded"
    );
    let (_d, st, _f, service) = setup(bytes.clone(), manifest_for(&bytes, json!({})));
    let status = tauri::async_runtime::block_on(service.check_now(&st, false)).unwrap();
    assert_eq!(
        status.offer.unwrap().decision,
        pos_core::updates::Decision::Offer
    );
    assert!(status.last_check_at.is_some());
}

#[test]
fn a_download_that_fails_verification_is_dropped_and_cannot_be_installed() {
    let announced = b"the real setup".to_vec();
    let (_d, st, fake, service) =
        setup(b"a cut setu".to_vec(), manifest_for(&announced, json!({})));
    tauri::async_runtime::block_on(service.check_now(&st, true)).unwrap();
    let error = tauri::async_runtime::block_on(service.download(&st, |_, _| {})).unwrap_err();
    assert_eq!(error.code(), "update_corrupt");
    assert_eq!(
        service.status(&st).unwrap().download,
        DownloadState::Failed {
            code: "update_corrupt".into()
        }
    );
    let session = st.session().unwrap();
    assert_eq!(
        service.install(&st, session.actor()).unwrap_err().code(),
        "update_not_ready"
    );
    assert!(fake.installed.lock().unwrap().is_empty());
    assert_eq!(
        count(
            &st,
            "SELECT COUNT(*) FROM audit_log WHERE action LIKE 'update.%'"
        ),
        0
    );
}

#[test]
fn a_signature_error_is_reported_as_update_corrupt() {
    use tauri_plugin_updater::Error;
    assert_eq!(
        map_plugin_error(&Error::MissingSignedVersion).code(),
        "update_corrupt"
    );
    assert_eq!(
        map_plugin_error(&Error::SignatureUtf8("x".into())).code(),
        "update_corrupt"
    );
    assert_eq!(
        map_plugin_error(&Error::Network("offline".into())).code(),
        "update_check_failed"
    );
    assert_eq!(
        map_plugin_error(&Error::ReleaseNotFound).code(),
        "update_check_failed"
    );
}

#[test]
fn install_is_refused_while_a_session_is_open_and_nothing_is_backed_up_or_journaled() {
    let bytes = b"the signed setup".to_vec();
    let (_d, st, fake, service) = setup(bytes.clone(), manifest_for(&bytes, json!({})));
    tauri::async_runtime::block_on(service.check_now(&st, true)).unwrap();
    tauri::async_runtime::block_on(service.download(&st, |_, _| {})).unwrap();
    open_drawer(&st);
    let backups_before = st.db.list_backups().unwrap().len();
    let session = st.session().unwrap();
    assert_eq!(
        service.install(&st, session.actor()).unwrap_err().code(),
        "update_blocked"
    );
    assert_eq!(service.status(&st).unwrap().blockers.len(), 1);
    assert_eq!(
        st.db.list_backups().unwrap().len(),
        backups_before,
        "no backup was taken"
    );
    assert_eq!(
        count(
            &st,
            "SELECT COUNT(*) FROM audit_log WHERE action = 'update.install_started'"
        ),
        0
    );
    assert!(fake.installed.lock().unwrap().is_empty());
    close_drawer(&st);
    service.install(&st, session.actor()).unwrap();
    assert_eq!(fake.installed.lock().unwrap().as_slice(), ["9.9.9"]);
    let pre = st
        .db
        .list_backups()
        .unwrap()
        .into_iter()
        .find(|b| b.kind == pos_core::db::BackupKind::PreMigration)
        .expect("a pre-update copy");
    let payload: String = st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT payload FROM audit_log WHERE action = 'update.install_started'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert!(
        payload.contains(&pre.file_name) && payload.contains("9.9.9"),
        "{payload}"
    );
}

#[test]
fn the_database_is_closed_before_the_setup_starts() {
    let bytes = b"the signed setup".to_vec();
    let (_d, st, _fake, service) = setup(bytes.clone(), manifest_for(&bytes, json!({})));
    tauri::async_runtime::block_on(service.check_now(&st, true)).unwrap();
    tauri::async_runtime::block_on(service.download(&st, |_, _| {})).unwrap();
    service.install(&st, Actor::system(None)).unwrap();
    let refused = st
        .db
        .write(|tx| pos_core::db::meta_set(tx, "after", "install"));
    assert_eq!(
        refused.unwrap_err().code(),
        "internal_error",
        "no write lands behind the clean-shutdown mark"
    );
}

#[test]
fn an_automatic_download_waits_for_the_drawer_to_close() {
    let bytes = b"the signed setup".to_vec();
    let (_d, st, _fake, service) = setup(bytes.clone(), manifest_for(&bytes, json!({})));
    open_drawer(&st);
    tauri::async_runtime::block_on(service.auto_step(&st)).unwrap();
    let status = service.status(&st).unwrap();
    assert!(status.offer.is_some(), "the check ran");
    assert_eq!(
        status.download,
        DownloadState::Idle,
        "no download during the shift"
    );
    close_drawer(&st);
    tauri::async_runtime::block_on(service.auto_step(&st)).unwrap();
    assert_eq!(service.status(&st).unwrap().download, DownloadState::Ready);
    assert_eq!(
        count(
            &st,
            "SELECT COUNT(*) FROM audit_log WHERE action = 'update.downloaded'"
        ),
        1
    );
}

/// Finding 13: a release paused after its download is never installed; the bytes go with it.
#[test]
fn a_release_paused_after_its_download_is_never_installed() {
    let bytes = b"the signed setup".to_vec();
    let (_d, st, fake, service) = setup(bytes.clone(), manifest_for(&bytes, json!({})));
    tauri::async_runtime::block_on(service.check_now(&st, true)).unwrap();
    tauri::async_runtime::block_on(service.download(&st, |_, _| {})).unwrap();
    *fake.manifest.lock().unwrap() = Some(manifest_for(&bytes, json!({ "paused": true })));
    let session = st.session().unwrap();
    assert_eq!(
        service.install(&st, session.actor()).unwrap_err().code(),
        "update_not_available"
    );
    let status = service.status(&st).unwrap();
    assert_eq!(
        status.download,
        DownloadState::Idle,
        "the bytes are dropped"
    );
    assert!(fake.installed.lock().unwrap().is_empty());
    assert_eq!(
        count(
            &st,
            "SELECT COUNT(*) FROM audit_log WHERE action = 'update.install_started'"
        ),
        0
    );
    assert_eq!(
        service.install(&st, session.actor()).unwrap_err().code(),
        "update_not_ready"
    );
    // Pulled from the channel altogether: the same.
    *fake.manifest.lock().unwrap() = Some(manifest_for(&bytes, json!({})));
    tauri::async_runtime::block_on(service.check_now(&st, true)).unwrap();
    tauri::async_runtime::block_on(service.download(&st, |_, _| {})).unwrap();
    *fake.manifest.lock().unwrap() = None;
    assert_eq!(
        service.install(&st, session.actor()).unwrap_err().code(),
        "update_not_available"
    );
}

/// Finding 13: offline at install time, a release installs only when a check answered recently.
#[test]
fn offline_at_install_time_a_recent_check_is_enough() {
    let bytes = b"the signed setup".to_vec();
    let (_d, st, fake, service) = setup(bytes.clone(), manifest_for(&bytes, json!({})));
    tauri::async_runtime::block_on(service.check_now(&st, true)).unwrap();
    tauri::async_runtime::block_on(service.download(&st, |_, _| {})).unwrap();
    fake.offline
        .store(true, std::sync::atomic::Ordering::SeqCst);
    service.install(&st, Actor::system(None)).unwrap();
    assert_eq!(fake.installed.lock().unwrap().as_slice(), ["9.9.9"]);
}

/// Finding 14: a download that stops receiving fails after `DOWNLOAD_STALL`, and frees the
/// service for the next check or download.
#[test]
fn a_stalled_download_fails_and_frees_the_service() {
    let bytes = b"the signed setup".to_vec();
    let (_d, st, fake, service) = setup(bytes.clone(), manifest_for(&bytes, json!({})));
    tauri::async_runtime::block_on(service.check_now(&st, true)).unwrap();
    fake.stall.store(true, std::sync::atomic::Ordering::SeqCst);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .build()
        .unwrap();
    let started = runtime.block_on(async { tokio::time::Instant::now() });
    let error = runtime
        .block_on(service.download(&st, |_, _| {}))
        .unwrap_err();
    assert_eq!(error.code(), "update_check_failed");
    let waited = runtime.block_on(async { started.elapsed() });
    assert!(waited >= crate::updates::DOWNLOAD_STALL, "{waited:?}");
    assert_eq!(
        service.status(&st).unwrap().download,
        DownloadState::Failed {
            code: "update_check_failed".into()
        }
    );
    fake.stall.store(false, std::sync::atomic::Ordering::SeqCst);
    tauri::async_runtime::block_on(service.download(&st, |_, _| {})).unwrap();
    assert_eq!(service.status(&st).unwrap().download, DownloadState::Ready);
}

/// A main PC that answers `/health` with `version`, on 127.0.0.1 (one thread, any number of
/// requests); returns its address.
fn main_pc_running(version: &'static str) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request);
            let body = json!({
                "hostId": "0190a000-0000-7000-8000-0000000000a1",
                "shopName": "Supérette El Amel",
                "appVersion": version,
                "schemaVersion": pos_core::db::migrations::LATEST_VERSION,
                "hostTime": pos_core::time::now_ms(),
                "pairingOpen": false,
                "suggestedCode": "C3",
            })
            .to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    address
}

/// This PC as a till of the main PC at `address` (only the place the update decision reads).
fn as_till_of(st: &AppState, address: String) {
    st.db
        .write(move |tx| {
            tx.execute(
                "UPDATE lan_local SET role = 'client', host_id = ?1, host_name = 'Principal',
                     host_address = ?2, paired_at = 1 WHERE id = 1",
                ["0190a000-0000-7000-8000-0000000000a1", address.as_str()],
            )?;
            Ok(())
        })
        .unwrap();
}

/// M6 review (finding 12): a till is offered only the version its main PC runs, whatever its
/// own install id and the rollout: never ahead of its main PC, and its main PC's own version
/// even outside the rollout.
#[test]
fn a_till_is_offered_only_the_version_its_main_pc_runs() {
    let bytes = b"the signed setup".to_vec();
    let (_d, st, _fake, service) =
        setup(bytes.clone(), manifest_for(&bytes, json!({ "rollout": 0 })));
    as_till_of(&st, main_pc_running("0.7.0"));
    let status = tauri::async_runtime::block_on(service.check_now(&st, true)).unwrap();
    assert_eq!(
        status.offer.unwrap().decision,
        pos_core::updates::Decision::MainPcFirst {
            version: Some("0.7.0".into())
        }
    );
    assert_eq!(
        tauri::async_runtime::block_on(service.download(&st, |_, _| {}))
            .unwrap_err()
            .code(),
        "update_not_available"
    );
    let (_d, st, _fake, service) =
        setup(bytes.clone(), manifest_for(&bytes, json!({ "rollout": 0 })));
    as_till_of(&st, main_pc_running("9.9.9"));
    let status = tauri::async_runtime::block_on(service.check_now(&st, true)).unwrap();
    assert_eq!(
        status.offer.unwrap().decision,
        pos_core::updates::Decision::Offer,
        "the main PC runs it: the till follows, outside the rollout"
    );
}

/// What the main PC mirrors is checked by the till as the plugin checks a download: a
/// setup signed for this version passes; an altered one, one announced as another version, or a
/// signature from another key is `update_corrupt`; the digest and size come on top.
#[test]
fn a_mirrored_setup_must_carry_the_updater_signature() {
    use crate::updates::verify_signed;
    // A throwaway test key (never the app's): it signed "the signed setup" for version 9.9.9.
    const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDU5MTYyMzAzRjRFMUJDOEEKUldTS3ZPSDBBeU1XV2NWczBjZ01ybGUrZWNhdVhtUkcybStUekZwK2s1UWtLUGhZSmhodjBVbGYK";
    const SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTS3ZPSDBBeU1XV1p4dTVoaDVnQWJxZVdIN0FOWVd2QmN2QjZtUWt2Q2JSWGF5QnNJeVIyN2NVZ3pQTm9HaldSdnBlYkZ3NnZnVTBycE5xQXdsUjJNandLLy9FZ3NzdFFBPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkwOTUzNDYyCWZpbGU6c2V0dXAuZXhlCXZlcnNpb246OS45LjkKOW9vWDVramFCcnJxSTVnRGkvdlJjR0xLZVM0NGtYQ0l5WFc2MzI4RjNCZDFWWGFSUlV3QWg2eWlxVmpnWkEzNHc3dVRrWUdMWWl4QmplOFRNaDQ2Q0E9PQo=";
    verify_signed(b"the signed setup", SIGNATURE, PUBKEY, "9.9.9").unwrap();
    let corrupt = |bytes: &[u8], version: &str, key: &str| {
        verify_signed(bytes, SIGNATURE, key, version)
            .unwrap_err()
            .code()
    };
    assert_eq!(
        corrupt(b"the signed setuP", "9.9.9", PUBKEY),
        "update_corrupt"
    );
    assert_eq!(
        corrupt(b"the signed setup", "9.9.10", PUBKEY),
        "update_corrupt",
        "an older build announced as a newer one"
    );
    let conf: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(crate::contract_tests::manifest_dir().join("tauri.conf.json"))
            .unwrap(),
    )
    .unwrap();
    let app_key = conf["plugins"]["updater"]["pubkey"].as_str().unwrap();
    assert_eq!(
        corrupt(b"the signed setup", "9.9.9", app_key),
        "update_corrupt",
        "not signed by the app's key"
    );
    let extras =
        pos_core::updates::ManifestExtras::from_raw(&manifest_for(b"the signed setup", json!({})));
    assert_eq!(
        pos_core::updates::check_artifact(b"the signed setuP", &extras)
            .unwrap_err()
            .code(),
        "update_corrupt"
    );
}
