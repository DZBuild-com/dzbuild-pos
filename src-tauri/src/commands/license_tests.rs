//! The licence commands: the status before the setup, activation from text and from a `.dzlic`,
//! the refusals, a trial's countdown, removal by the owner with the PIN, every state on screen.

use super::license::{self, LicenseStateKind, LicenseStatus};
use super::tests::first_run;
use crate::state::AppState;
use pos_core::error::AppError;
use pos_core::time::now_ms;
use pos_license::codes;
use pos_license::payload::{DAY_MS, LicenseKind};
use pos_license::state::{Licence, LicenseState};
use pos_license::testkit;

fn license_actions(st: &AppState) -> Vec<String> {
    st.db
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

#[test]
fn the_status_before_setup_shows_this_pc_and_no_licence() {
    let st = AppState::for_tests();
    let status = license::status(&st);
    assert_eq!(status.state, LicenseStateKind::Unlicensed);
    assert!(!status.can_write);
    assert_eq!(status.refusal.as_deref(), Some(codes::REQUIRED));
    assert_eq!(
        status.device_code.as_deref(),
        Some(testkit::device().code())
    );
    assert!(
        status.qr.is_some(),
        "the QR the customer scans with a phone"
    );
    assert!(status.licence.is_none() && status.days_left.is_none() && !status.warn);
}

#[test]
fn activation_from_text_and_from_a_file_journals_and_opens_the_gate() {
    let st = AppState::for_tests();
    let device = testkit::device();
    let status = license::activate(&st, testkit::trial(&device, now_ms())).unwrap();
    assert_eq!(status.state, LicenseStateKind::Active);
    assert!(status.can_write && status.refusal.is_none());
    assert_eq!(status.days_left, Some(7));
    assert!(crate::license::verdict("sales_create", Some(&st)).is_ok());
    // a lifetime licence in a .dzlic, cut over two lines by a mail client
    let dir = tempfile::tempdir().unwrap();
    let lifetime = testkit::lifetime(&device, now_ms());
    let (a, b) = lifetime.split_at(60);
    let file = dir.path().join("Supérette El Amel.dzlic");
    std::fs::write(
        &file,
        format!("# DZBuild POS licence (lifetime)\r\n{a}\r\n{b}\r\n"),
    )
    .unwrap();
    let status = license::import_from(&st, &file).unwrap();
    assert_eq!(
        status.licence.as_ref().map(|l| l.kind),
        Some(LicenseKind::Lifetime)
    );
    assert_eq!(status.days_left, None);
    assert_eq!(
        license_actions(&st),
        ["license.activated", "license.activated"]
    );
}

#[test]
fn a_trial_counts_whole_days_and_warns_from_three() {
    let now = now_ms();
    let st = AppState::for_tests();
    license::activate(&st, testkit::trial(&testkit::device(), now - 4 * DAY_MS)).unwrap();
    let status = license::status(&st);
    assert_eq!((status.days_left, status.warn), (Some(3), true));
    let st = AppState::for_tests();
    license::activate(&st, testkit::trial(&testkit::device(), now - 2 * DAY_MS)).unwrap();
    let status = license::status(&st);
    assert_eq!((status.days_left, status.warn), (Some(5), false));
}

#[test]
fn activation_refusals_surface_their_codes_and_write_nothing() {
    let st = AppState::for_tests();
    let code = |r: Result<LicenseStatus, AppError>| r.unwrap_err().code();
    let theirs = testkit::lifetime(&testkit::other_device(), now_ms());
    assert_eq!(code(license::activate(&st, theirs)), codes::WRONG_DEVICE);
    assert_eq!(
        code(license::activate(&st, "bonjour".into())),
        codes::INVALID
    );
    assert_eq!(
        code(license::activate(&st, "A".repeat(5_000))),
        codes::INVALID
    );
    let dir = tempfile::tempdir().unwrap();
    let big = dir.path().join("gros.dzlic");
    std::fs::write(&big, vec![b'#'; 9_000]).unwrap();
    assert_eq!(code(license::import_from(&st, &big)), codes::INVALID);
    assert_eq!(license::status(&st).state, LicenseStateKind::Unlicensed);
    assert!(license_actions(&st).is_empty());
}

#[test]
fn removal_needs_the_owner_and_the_pin() {
    let st = AppState::for_tests();
    let owner = first_run(&st);
    license::activate(&st, testkit::lifetime(&testkit::device(), now_ms())).unwrap();
    assert_eq!(
        license::remove(&st, "1111".into()).unwrap_err().code(),
        "invalid_pin"
    );
    assert!(license::status(&st).can_write, "nothing removed");
    let status = license::remove(&st, "2580".into()).unwrap();
    assert_eq!(status.state, LicenseStateKind::Unlicensed);
    let who: Option<String> = st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT user_id FROM audit_log WHERE action = 'license.removed'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(who, Some(owner.user_id));
}

#[test]
fn every_state_reads_on_screen() {
    let code = testkit::device().code().to_owned();
    let licence = Licence {
        id: "0199a0b0-0000-7000-8000-00000000a001".into(),
        kind: LicenseKind::Trial,
        issued_at: 0,
        expires_at: Some(10 * DAY_MS),
        customer: None,
        activated_at: Some(0),
    };
    // A real code (valid check symbol), the one `src/test/fixtures-m9.ts` shows as the previous PC.
    let other = "DZPOS-0A1B-2C3D-4E5F-6G73".to_owned();
    assert_eq!(
        pos_license::device::parse_code(&other).as_deref(),
        Some(other.as_str())
    );
    let cases = [
        (LicenseState::Demo, LicenseStateKind::Demo, None, true),
        (
            LicenseState::Unlicensed,
            LicenseStateKind::Unlicensed,
            Some(codes::REQUIRED),
            false,
        ),
        (
            LicenseState::Active(licence.clone()),
            LicenseStateKind::Active,
            None,
            true,
        ),
        (
            LicenseState::Expired(licence),
            LicenseStateKind::Expired,
            Some(codes::EXPIRED),
            false,
        ),
        (
            LicenseState::Refused {
                code: codes::REVOKED,
            },
            LicenseStateKind::Refused,
            Some(codes::REVOKED),
            false,
        ),
        (
            LicenseState::OtherDevice {
                previous: other.clone(),
            },
            LicenseStateKind::OtherDevice,
            Some(codes::WRONG_DEVICE),
            false,
        ),
        (
            LicenseState::Tampered,
            LicenseStateKind::Tampered,
            Some(codes::TAMPERED),
            false,
        ),
        (
            LicenseState::DeviceUnreadable,
            LicenseStateKind::DeviceUnreadable,
            Some(codes::DEVICE_UNREADABLE),
            false,
        ),
    ];
    for (state, kind, refusal, can_write) in cases {
        let status = license::status_of(&state, Some(&code), 0);
        assert_eq!(
            (status.state, status.refusal.as_deref(), status.can_write),
            (kind, refusal, can_write),
            "{state:?}"
        );
    }
    let moved = license::status_of(
        &LicenseState::OtherDevice {
            previous: other.clone(),
        },
        Some(&code),
        0,
    );
    assert_eq!(moved.previous_device, Some(other));
    assert!(
        license::status_of(&LicenseState::DeviceUnreadable, None, 0)
            .qr
            .is_none()
    );
}
