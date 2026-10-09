//! The engine on an in-memory shop: activation, trials and the clock, one trial per device,
//! removal, repairs after a restore, tampering, and logs that never carry licence material.
//! Licences are issued relative to `now_ms()`: the journal is dated with the real clock.

use crate::codes;
use crate::device::DeviceId;
use crate::engine::{self, Context, Outcome};
use crate::keys::Keyring;
use crate::payload::{DAY_MS, LicenseKind};
use crate::record::Record;
use crate::state::LicenseState;
use crate::store::{ACCOUNT, META_KEY};
use crate::testkit::{self, FailingStore};
use pos_core::audit::Actor;
use pos_core::cloud::secrets::{MemorySecrets, SecretStore};
use pos_core::db::Db;
use pos_core::error::AppError;
use pos_core::time::now_ms;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Shop {
    db: Db,
    vault: MemorySecrets,
    file: MemorySecrets,
    device: DeviceId,
    keyring: Keyring,
    actor: Actor,
}

impl Shop {
    fn new() -> Shop {
        let db = Db::open_memory().unwrap();
        let terminal = pos_core::setup::bootstrap(&db).unwrap();
        Shop {
            db,
            vault: MemorySecrets::default(),
            file: MemorySecrets::default(),
            device: testkit::device(),
            keyring: testkit::keyring(),
            actor: Actor::system(Some(terminal.id)),
        }
    }

    /// A new, empty database on the same PC (the old `pos.db` deleted); the PC's own stores stay.
    fn with_new_database(self) -> Shop {
        let db = Db::open_memory().unwrap();
        let terminal = pos_core::setup::bootstrap(&db).unwrap();
        Shop {
            db,
            actor: Actor::system(Some(terminal.id)),
            ..self
        }
    }

    fn ctx(&self, now: i64) -> Context<'_> {
        Context {
            db: &self.db,
            vault: &self.vault,
            file: &self.file,
            device: &self.device,
            keyring: &self.keyring,
            actor: &self.actor,
            now,
        }
    }

    fn actions(&self) -> Vec<String> {
        self.db
            .read(|c| {
                let mut stmt = c.prepare(
                    "SELECT action FROM audit_log WHERE action LIKE 'license.%' ORDER BY seq",
                )?;
                let rows = stmt
                    .query_map([], |r| r.get(0))?
                    .collect::<Result<Vec<String>, _>>()?;
                Ok(rows)
            })
            .unwrap()
    }

    fn last_payload(&self, action: &'static str, path: &'static str) -> String {
        self.db
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT CAST(json_extract(payload, ?2) AS TEXT) FROM audit_log
                     WHERE action = ?1 ORDER BY seq DESC LIMIT 1",
                    [action, path],
                    |r| r.get::<_, Option<String>>(0),
                )?)
            })
            .unwrap()
            .unwrap_or_default()
    }

    fn meta(&self) -> Option<String> {
        self.db
            .read(|c| pos_core::db::meta_get(c, META_KEY))
            .unwrap()
    }

    /// A backup from before the activation restored, or a till's snapshot installed.
    fn drop_meta(&self) {
        self.db
            .write(|tx| {
                tx.execute("DELETE FROM app_meta WHERE key = ?1", [META_KEY])?;
                Ok(())
            })
            .unwrap();
    }
}

fn code(result: Result<Outcome, AppError>) -> String {
    result.err().map(|e| e.code()).unwrap_or_default()
}

fn lifetime_active(state: &LicenseState) -> bool {
    matches!(state, LicenseState::Active(l) if l.kind == LicenseKind::Lifetime)
}

#[test]
fn a_trial_activates_counts_down_and_expires_once_against_the_newest_time_seen() {
    let shop = Shop::new();
    let now = now_ms();
    let key = testkit::trial(&shop.device, now - DAY_MS);
    let outcome = engine::activate(&shop.ctx(now), &key).unwrap();
    let LicenseState::Active(licence) = &outcome.state else {
        panic!("{:?}", outcome.state)
    };
    assert_eq!(licence.days_left(now), Some(6));
    assert!(shop.meta().is_some(), "app_meta");
    assert!(
        shop.vault.get(ACCOUNT).unwrap().is_some(),
        "Credential Manager"
    );
    assert!(
        shop.file.get(ACCOUNT).unwrap().is_some(),
        "LOCALAPPDATA file"
    );
    assert!(
        engine::refresh(&shop.ctx(now + 2 * DAY_MS), true)
            .unwrap()
            .state
            .allows_writes()
    );
    let end = now + 6 * DAY_MS;
    let ended = engine::refresh(&shop.ctx(end + 60_000), true).unwrap();
    assert!(
        matches!(ended.state, LicenseState::Expired(_)),
        "{:?}",
        ended.state
    );
    engine::refresh(&shop.ctx(end + 120_000), true).unwrap();
    assert_eq!(
        shop.actions(),
        ["license.activated", "license.expired"],
        "the end is journaled once"
    );
    // the clock set back to the day of the activation: still expired
    let back = engine::refresh(&shop.ctx(now), true).unwrap();
    assert!(
        matches!(back.state, LicenseState::Expired(_)),
        "{:?}",
        back.state
    );
    assert_eq!(shop.last_payload("license.activated", "$.kind"), "trial");
    assert_eq!(
        shop.last_payload("license.activated", "$.customer"),
        "",
        "no customer in the journal"
    );
}

#[test]
fn a_clock_set_back_cannot_revive_a_trial_even_on_a_new_database() {
    let shop = Shop::new();
    let now = now_ms();
    let key = testkit::trial(&shop.device, now - DAY_MS);
    engine::activate(&shop.ctx(now), &key).unwrap();
    engine::refresh(&shop.ctx(now + 7 * DAY_MS), true).unwrap(); // this PC was seen past the end
    let shop = shop.with_new_database();
    let fresh = engine::refresh(&shop.ctx(now), true).unwrap();
    assert!(
        matches!(fresh.state, LicenseState::Expired(_)),
        "{:?}",
        fresh.state
    );
    assert_eq!(code(engine::activate(&shop.ctx(now), &key)), codes::EXPIRED);
    assert_eq!(shop.actions(), ["license.repaired", "license.expired"]);
}

/// Final review: a trial counted against the newest time seen could still be frozen by setting the
/// clock back again and again before its end. A clock behind the newest time this PC showed (more
/// than a time-service correction) now stops a trial's writes until the date is set; set right,
/// the trial goes on and ends on its own day.
#[test]
fn a_clock_set_back_during_a_trial_cannot_freeze_it() {
    let shop = Shop::new();
    let now = now_ms();
    let key = testkit::trial(&shop.device, now - DAY_MS);
    engine::activate(&shop.ctx(now), &key).unwrap();
    let day5 = now + 4 * DAY_MS;
    assert!(
        engine::refresh(&shop.ctx(day5), true)
            .unwrap()
            .state
            .allows_writes()
    );
    for back in [
        now,
        now + DAY_MS,
        day5 - 2 * crate::state::CLOCK_TOLERANCE_MS,
    ] {
        let outcome = engine::refresh(&shop.ctx(back), true).unwrap();
        assert_eq!(
            outcome.state.refusal().map(|e| e.code()).as_deref(),
            Some(codes::CLOCK_BEHIND),
            "{back}: {:?}",
            outcome.state
        );
    }
    assert_eq!(
        code(engine::activate(&shop.ctx(now), &key)),
        codes::CLOCK_BEHIND,
        "activating the trial again does not help"
    );
    assert!(
        engine::refresh(&shop.ctx(day5 - 60_000), true)
            .unwrap()
            .state
            .allows_writes(),
        "a minute of Windows' time correction is tolerated"
    );
    let end = now + 6 * DAY_MS;
    assert!(
        engine::refresh(&shop.ctx(end - 60_000), true)
            .unwrap()
            .state
            .allows_writes()
    );
    let ended = engine::refresh(&shop.ctx(end + 1), true).unwrap();
    assert!(
        matches!(ended.state, LicenseState::Expired(_)),
        "{:?}",
        ended.state
    );
}

#[test]
fn a_lifetime_never_expires_whatever_the_clock_does() {
    let shop = Shop::new();
    let now = now_ms();
    engine::activate(&shop.ctx(now), &testkit::lifetime(&shop.device, now)).unwrap();
    // a dead CMOS battery (2016), then ten years on
    for at in [now - 3_650 * DAY_MS, now + 3_650 * DAY_MS] {
        let outcome = engine::refresh(&shop.ctx(at), true).unwrap();
        assert!(lifetime_active(&outcome.state), "{at}: {:?}", outcome.state);
    }
    assert_eq!(
        shop.actions(),
        ["license.activated"],
        "nothing else to journal"
    );
}

#[test]
fn one_trial_per_device_even_after_removal_or_wiped_stores() {
    let shop = Shop::new();
    let now = now_ms();
    let first = testkit::trial(&shop.device, now);
    let second = testkit::trial(&shop.device, now);
    engine::activate(&shop.ctx(now), &first).unwrap();
    assert_eq!(
        code(engine::activate(&shop.ctx(now), &second)),
        codes::TRIAL_USED
    );
    engine::remove(&shop.ctx(now)).unwrap();
    assert_eq!(
        code(engine::activate(&shop.ctx(now), &second)),
        codes::TRIAL_USED,
        "removing the licence keeps the mark"
    );
    shop.drop_meta();
    shop.vault.delete(ACCOUNT).unwrap();
    shop.file.delete(ACCOUNT).unwrap();
    assert_eq!(
        code(engine::activate(&shop.ctx(now), &second)),
        codes::TRIAL_USED,
        "every store wiped: the journal is the witness"
    );
    let again = engine::activate(&shop.ctx(now + DAY_MS), &first).unwrap();
    assert!(
        again.state.allows_writes(),
        "the first trial itself comes back, with its own end"
    );
}

#[test]
fn a_lifetime_replaces_a_trial_and_a_trial_never_replaces_a_lifetime() {
    let shop = Shop::new();
    let now = now_ms();
    let trial = testkit::trial(&shop.device, now);
    engine::activate(&shop.ctx(now), &trial).unwrap();
    let outcome = engine::activate(&shop.ctx(now), &testkit::lifetime(&shop.device, now)).unwrap();
    assert!(lifetime_active(&outcome.state), "{:?}", outcome.state);
    let trial_id = crate::verify::verify(&trial, &shop.keyring, shop.device.code())
        .unwrap()
        .payload
        .id;
    assert_eq!(
        shop.last_payload("license.activated", "$.replaced"),
        trial_id
    );
    assert_eq!(
        code(engine::activate(&shop.ctx(now), &trial)),
        codes::DOWNGRADE
    );
}

#[test]
fn refusals_at_activation_write_nothing() {
    let shop = Shop::new();
    let now = now_ms();
    let theirs = testkit::lifetime(&testkit::other_device(), now);
    assert_eq!(
        code(engine::activate(&shop.ctx(now), &theirs)),
        codes::WRONG_DEVICE
    );
    assert_eq!(
        code(engine::activate(&shop.ctx(now), "bonjour")),
        codes::INVALID
    );
    let from_the_future = testkit::lifetime(&shop.device, now + 3 * DAY_MS);
    assert_eq!(
        code(engine::activate(&shop.ctx(now), &from_the_future)),
        codes::CLOCK_BEHIND
    );
    let old_trial = testkit::trial(&shop.device, now - 10 * DAY_MS);
    assert_eq!(
        code(engine::activate(&shop.ctx(now), &old_trial)),
        codes::EXPIRED
    );
    assert!(shop.meta().is_none());
    assert!(
        shop.vault.get(ACCOUNT).unwrap().is_none() && shop.file.get(ACCOUNT).unwrap().is_none()
    );
    assert!(shop.actions().is_empty());
}

#[test]
fn removal_keeps_the_trial_mark_and_is_journaled() {
    let shop = Shop::new();
    let now = now_ms();
    engine::activate(&shop.ctx(now), &testkit::lifetime(&shop.device, now)).unwrap();
    assert_eq!(
        engine::remove(&shop.ctx(now)).unwrap().state,
        LicenseState::Unlicensed
    );
    assert_eq!(
        engine::refresh(&shop.ctx(now), true).unwrap().state,
        LicenseState::Unlicensed
    );
    assert_eq!(shop.actions(), ["license.activated", "license.removed"]);
    assert_eq!(shop.last_payload("license.removed", "$.kind"), "lifetime");
    let record: Record = serde_json::from_str(&shop.file.get(ACCOUNT).unwrap().unwrap()).unwrap();
    assert!(record.key.is_none());
    assert_eq!(
        engine::remove(&shop.ctx(now)).unwrap().state,
        LicenseState::Unlicensed
    );
    assert_eq!(shop.actions().len(), 2, "removing twice journals once");
}

#[test]
fn stores_heal_after_a_restore_or_a_till_snapshot_and_journal_the_repair() {
    let shop = Shop::new();
    let now = now_ms();
    engine::activate(&shop.ctx(now), &testkit::lifetime(&shop.device, now)).unwrap();
    shop.drop_meta();
    assert!(
        engine::refresh(&shop.ctx(now), true)
            .unwrap()
            .state
            .allows_writes()
    );
    assert!(shop.meta().is_some(), "the database copy is back");
    shop.vault.delete(ACCOUNT).unwrap(); // the Credential Manager was reset
    engine::refresh(&shop.ctx(now), true).unwrap();
    assert!(shop.vault.get(ACCOUNT).unwrap().is_some());
    assert_eq!(
        shop.actions(),
        ["license.activated", "license.repaired", "license.repaired"]
    );
    assert_eq!(
        shop.last_payload("license.repaired", "$.stores"),
        "[\"vault\"]"
    );
    // a Credential Manager Windows will not open: left out, never tampered, nothing journaled
    let locked = Context {
        vault: &FailingStore,
        ..shop.ctx(now)
    };
    assert!(
        engine::refresh(&locked, true)
            .unwrap()
            .state
            .allows_writes()
    );
    assert_eq!(shop.actions().len(), 3);
}

#[test]
fn an_edited_store_is_tampered_journaled_once_and_a_new_activation_clears_it() {
    let shop = Shop::new();
    let now = now_ms();
    let key = testkit::lifetime(&shop.device, now);
    engine::activate(&shop.ctx(now), &key).unwrap();
    let mut value: serde_json::Value =
        serde_json::from_str(&shop.file.get(ACCOUNT).unwrap().unwrap()).unwrap();
    value["seenAt"] = serde_json::json!(0);
    shop.file.set(ACCOUNT, &value.to_string()).unwrap();
    assert_eq!(
        engine::refresh(&shop.ctx(now), true).unwrap().state,
        LicenseState::Tampered
    );
    engine::refresh(&shop.ctx(now), false).unwrap(); // the shell journals once per run
    assert_eq!(shop.actions(), ["license.activated", "license.tampered"]);
    let problems = shop.last_payload("license.tampered", "$.problems");
    assert!(
        problems.contains("\"file\"") && problems.contains("\"edited\""),
        "{problems}"
    );
    assert!(
        engine::activate(&shop.ctx(now), &key)
            .unwrap()
            .state
            .allows_writes()
    );
    assert!(
        engine::refresh(&shop.ctx(now), true)
            .unwrap()
            .state
            .allows_writes(),
        "fresh copies everywhere: clean again"
    );
}

/// Without the engine's lock, the stale write showed up in round 0 or 1 of every run.
const RACE_ROUNDS: usize = 12;

/// Runs `act` on this thread while the 10-minute check runs again and again on another one, the
/// way the shell's refresher runs beside the licence commands. Each check is a second later than
/// the one before, so that a trial's record is written every time (and the trial stays far from
/// its end).
fn beside_the_checks(shop: &Shop, clock: &AtomicI64, act: impl FnOnce()) {
    let checks = AtomicUsize::new(0);
    let acting = AtomicBool::new(false);
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                let at = clock.fetch_add(1_000, Ordering::SeqCst) + 1_000;
                engine::refresh(&shop.ctx(at), true).unwrap();
                checks.fetch_add(1, Ordering::SeqCst);
                if acting.load(Ordering::SeqCst) {
                    // A lock is not fair: a pause lets the command in instead of starving it.
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        });
        while checks.load(Ordering::SeqCst) == 0 {
            std::thread::yield_now();
        }
        acting.store(true, Ordering::SeqCst);
        act();
        done.store(true, Ordering::SeqCst);
    });
}

/// The key each copy holds (the database, the Credential Manager, the file).
fn keys_held(shop: &Shop) -> [Option<String>; 3] {
    let key_of = |text: Option<String>| {
        text.and_then(|text| serde_json::from_str::<Record>(&text).unwrap().key)
    };
    [
        key_of(shop.meta()),
        key_of(shop.vault.get(ACCOUNT).unwrap()),
        key_of(shop.file.get(ACCOUNT).unwrap()),
    ]
}

/// A check that read the copies just before an activation never writes its stale record back
/// afterwards: the lifetime licence bought to replace the trial stays.
#[test]
fn a_check_beside_an_activation_never_writes_back_the_licence_it_replaced() {
    let shop = Shop::new();
    let now = now_ms();
    let trial = testkit::trial(&shop.device, now);
    let lifetime = testkit::lifetime(&shop.device, now);
    let clock = AtomicI64::new(now);
    for round in 0..RACE_ROUNDS {
        engine::remove(&shop.ctx(now)).unwrap();
        engine::activate(&shop.ctx(now), &trial).unwrap();
        beside_the_checks(&shop, &clock, || {
            engine::activate(&shop.ctx(now), &lifetime).unwrap();
        });
        let at = clock.load(Ordering::SeqCst);
        let after = engine::refresh(&shop.ctx(at), true).unwrap();
        assert!(
            lifetime_active(&after.state),
            "round {round}: the activation was overwritten: {:?}",
            after.state
        );
        assert_eq!(
            keys_held(&shop),
            [
                Some(lifetime.clone()),
                Some(lifetime.clone()),
                Some(lifetime.clone())
            ],
            "round {round}: every copy holds the lifetime licence"
        );
    }
    assert!(!shop.actions().iter().any(|a| a == "license.tampered"));
}

/// A check that read the copies just before a removal never writes its stale record back
/// afterwards: the removed licence never comes back.
#[test]
fn a_check_beside_a_removal_never_brings_the_licence_back() {
    let shop = Shop::new();
    let now = now_ms();
    let trial = testkit::trial(&shop.device, now);
    let clock = AtomicI64::new(now);
    for round in 0..RACE_ROUNDS {
        engine::activate(&shop.ctx(now), &trial).unwrap();
        beside_the_checks(&shop, &clock, || {
            engine::remove(&shop.ctx(now)).unwrap();
        });
        let at = clock.load(Ordering::SeqCst);
        assert_eq!(
            engine::refresh(&shop.ctx(at), true).unwrap().state,
            LicenseState::Unlicensed,
            "round {round}: the removed licence came back"
        );
        assert_eq!(keys_held(&shop), [None, None, None], "round {round}");
    }
    assert!(!shop.actions().iter().any(|a| a == "license.tampered"));
}

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Captured {
        self.clone()
    }
}

#[test]
fn the_licence_never_reaches_the_logs() {
    let captured = Captured::default();
    // No timestamp: its microseconds could spell one of the needles below by chance.
    let subscriber = tracing_subscriber::fmt()
        .with_writer(captured.clone())
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .without_time()
        .finish();
    let shop = Shop::new();
    let now = now_ms();
    let key = testkit::trial(&shop.device, now);
    tracing::subscriber::with_default(subscriber, || {
        // the vault refuses everything: the engine logs it (without content) and carries on
        let locked = Context {
            vault: &FailingStore,
            ..shop.ctx(now)
        };
        engine::activate(&locked, &key).unwrap();
        engine::refresh(&locked, true).unwrap();
        engine::remove(&locked).unwrap();
    });
    let logs = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    assert!(
        logs.contains("licence store could not"),
        "the capture works: {logs}"
    );
    let stored: serde_json::Value =
        serde_json::from_str(&shop.file.get(ACCOUNT).unwrap().unwrap()).unwrap();
    let seal = stored["seal"].as_str().unwrap().to_owned();
    let secret = hex::encode(shop.device.secret());
    for needle in [
        &key[..],
        &key[5..40],
        &seal[..],
        &secret[..],
        shop.device.code(),
        "6f1c3a52",
        "El Amel",
        "0550",
    ] {
        assert!(!logs.contains(needle), "{needle:.12}… reached the logs");
    }
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every `tracing::<level>!(…)` of the crate's code (tests left out) names no licence material,
/// neither as a field nor in its message.
#[test]
fn no_log_line_names_licence_material() {
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
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let mut calls = 0;
    for file in &files {
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name.ends_with("_tests.rs") || name == "testkit.rs" {
            continue;
        }
        let text = std::fs::read_to_string(file).unwrap();
        // the code before the first test item
        let code = text.split("#[cfg(test)]").next().unwrap_or_default();
        for (at, _) in code.match_indices("tracing::") {
            let rest = &code[at + "tracing::".len()..];
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
    assert!(calls >= 2, "the scan sees the crate's log lines ({calls})");
}
