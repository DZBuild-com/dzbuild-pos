//! The owner's tool on temporary folders (outside any git work tree: `%TEMP%`), with a fast age
//! work factor.

use super::*;
use crate::ledger::{ISSUED_DIR, LEDGER_FILE, Ledger};
use pos_license::device::{Components, DeviceId};
use pos_license::payload::{DAY_MS, LicenseKind};
use pos_license::testkit;
use std::path::Path;

const PASS: &str = "sable dune palmier thé";

struct FakeEnv {
    pass: Vec<String>,
    now: i64,
    out: Vec<u8>,
    keys: PathBuf,
    /// The keys "this release" trusts (none: the first keygen).
    release: Vec<(String, [u8; 32])>,
}

impl Env for FakeEnv {
    fn passphrase(&mut self, _prompt: &str) -> Result<String, CliError> {
        if self.pass.is_empty() {
            return Err(CliError::new("no passphrase typed"));
        }
        Ok(self.pass.remove(0))
    }
    fn now(&self) -> i64 {
        self.now
    }
    fn out(&mut self) -> &mut dyn Write {
        &mut self.out
    }
    fn default_keys_dir(&self) -> Option<PathBuf> {
        Some(self.keys.clone())
    }
    fn work_factor(&self) -> u8 {
        10
    }
    fn release_keys(&self) -> Vec<(String, [u8; 32])> {
        self.release.clone()
    }
}

fn env(dir: &Path) -> FakeEnv {
    FakeEnv {
        pass: vec![PASS.to_owned(); 8],
        now: 1_790_000_000_000,
        out: Vec::new(),
        keys: dir.join("licence key"),
        release: Vec::new(),
    }
}

fn run_with(env: &mut FakeEnv, args: &[&str]) -> Result<String, CliError> {
    env.out.clear();
    let argv: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    run(&argv, env)?;
    Ok(String::from_utf8(env.out.clone()).unwrap())
}

fn keyed(dir: &Path) -> FakeEnv {
    let mut e = env(dir);
    run_with(&mut e, &["keygen"]).unwrap();
    e.pass = vec![PASS.to_owned(); 8];
    e
}

fn key_in(output: &str) -> String {
    output
        .lines()
        .find(|l| l.starts_with("DZL1."))
        .unwrap()
        .to_owned()
}

fn ring_of(e: &FakeEnv) -> Keyring {
    let (id, public) = keyfile::read_public(&e.keys).unwrap();
    Keyring::default().with(&id, public)
}

fn third_device() -> DeviceId {
    DeviceId::from_components(&Components {
        product_id: "00330-80000-00000-ZZ999".into(),
        ..testkit::components()
    })
    .unwrap()
}

#[test]
fn keygen_seals_the_key_and_refuses_twice_short_mismatched_or_inside_a_repo() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = env(dir.path());
    let out = run_with(&mut e, &["keygen"]).unwrap();
    assert!(
        out.contains("PUBLIC_KEYS") && out.contains("KEYRING_SHA256"),
        "{out}"
    );
    let sealed = std::fs::read(e.keys.join(keyfile::SECRET_FILE)).unwrap();
    assert!(
        sealed.starts_with(b"age-encryption.org/v1"),
        "sealed with age"
    );
    let (id, public) = keyfile::read_public(&e.keys).unwrap();
    assert_eq!(id, "k1");
    assert!(out.contains(&pos_license::keys::keyring_digest_of(&[(
        "k1",
        public.to_bytes()
    )])));
    assert!(
        e.keys.join(LEDGER_FILE).is_file(),
        "an empty ledger beside the key"
    );
    let again = run_with(&mut e, &["keygen"]).unwrap_err();
    assert!(again.0.contains("never overwritten"), "{again}");

    let repo = dir.path().join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let inside = repo.join("keys");
    let mut e = env(dir.path());
    let refused = run_with(&mut e, &["keygen", "--keys", inside.to_str().unwrap()]).unwrap_err();
    assert!(refused.0.contains("git"), "{refused}");
    assert!(!inside.exists(), "nothing written");

    let mut e = env(dir.path());
    e.keys = dir.path().join("short");
    e.pass = vec!["court".into(), "court".into()];
    assert!(
        run_with(&mut e, &["keygen"])
            .unwrap_err()
            .0
            .contains("12 characters")
    );
    e.pass = vec![PASS.into(), "another passphrase".into()];
    assert!(
        run_with(&mut e, &["keygen"])
            .unwrap_err()
            .0
            .contains("differ")
    );
}

/// Final review: the release has its `k1`; keygen never makes another `k1` (it would refuse every
/// licence the first one signed) and a rotation's snippet appends to the keys, its digest over all.
#[test]
fn keygen_never_makes_a_key_the_release_has_and_a_rotation_appends() {
    let dir = tempfile::tempdir().unwrap();
    let first = keyed(dir.path());
    let (_, k1) = keyfile::read_public(&first.keys).unwrap();
    let mut e = env(dir.path());
    e.keys = dir.path().join("rotation");
    e.release = vec![("k1".to_owned(), k1.to_bytes())];
    let bare = run_with(&mut e, &["keygen"]).unwrap_err();
    assert!(
        bare.0.contains("already has its licence key") && bare.0.contains("--id k2"),
        "{bare}"
    );
    let again = run_with(&mut e, &["keygen", "--id", "k1"]).unwrap_err();
    assert!(again.0.contains("k1 is already a key"), "{again}");
    assert!(!e.keys.exists(), "nothing made");
    let out = run_with(&mut e, &["keygen", "--id", "k2"]).unwrap();
    assert!(out.contains("Append this key to PUBLIC_KEYS"), "{out}");
    assert!(!out.contains("replacing"), "{out}");
    assert!(
        out.contains("id: \"k2\"") && !out.contains("pub const PUBLIC_KEYS"),
        "{out}"
    );
    let (id, k2) = keyfile::read_public(&e.keys).unwrap();
    assert_eq!(id, "k2");
    let digest =
        pos_license::keys::keyring_digest_of(&[("k1", k1.to_bytes()), ("k2", k2.to_bytes())]);
    assert!(out.contains(&digest), "the digest of k1 then k2: {out}");
}

/// A `passphrase.txt` left beside the key is pointed out at each issue;
/// the tool never reads it.
#[test]
fn issue_warns_about_a_passphrase_file_beside_the_key() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = keyed(dir.path());
    let code = testkit::device().code().to_owned();
    let quiet = run_with(&mut e, &["issue", "--device", &code, "--kind", "trial"]).unwrap();
    assert!(!quiet.contains(PASSPHRASE_FILE), "{quiet}");
    std::fs::write(e.keys.join(PASSPHRASE_FILE), "test").unwrap();
    e.pass = vec![PASS.to_owned(); 4];
    let warned = run_with(&mut e, &["issue", "--device", &code, "--kind", "lifetime"]).unwrap();
    assert!(
        warned.contains("WARNING: passphrase.txt is in the key folder"),
        "{warned}"
    );
}

#[test]
fn a_wrong_passphrase_issues_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = keyed(dir.path());
    e.pass = vec!["wrong passphrase 123".into()];
    let code = testkit::device().code().to_owned();
    let err = run_with(&mut e, &["issue", "--device", &code, "--kind", "lifetime"]).unwrap_err();
    assert!(err.0.contains("wrong passphrase"), "{err}");
    assert!(Ledger::load(&e.keys).unwrap().licences.is_empty());
}

#[test]
fn issue_signs_a_key_the_app_accepts_and_keeps_the_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = keyed(dir.path());
    let device = testkit::device();
    // read over the phone: lower case, spaces
    let typed = device.code().to_lowercase().replace('-', " ");
    let out = run_with(
        &mut e,
        &[
            "issue",
            "--device",
            &typed,
            "--kind",
            "trial",
            "--customer",
            "Supérette El Amel",
            "--phone",
            "0550 12 34 56",
        ],
    )
    .unwrap();
    let text = key_in(&out);
    let verified = verify(&text, &ring_of(&e), device.code()).unwrap();
    assert_eq!(verified.payload.kind, LicenseKind::Trial);
    assert_eq!(verified.payload.issued_at, e.now);
    assert_eq!(verified.payload.expires_at, Some(e.now + 7 * DAY_MS));
    assert_eq!(
        verified.payload.customer.and_then(|c| c.name).as_deref(),
        Some("Supérette El Amel")
    );
    let ledger = Ledger::load(&e.keys).unwrap();
    assert_eq!(ledger.licences.len(), 1);
    assert_eq!(ledger.licences[0].device_code, device.code());
    let file = e
        .keys
        .join(ISSUED_DIR)
        .join(format!("{}.dzlic", verified.payload.id));
    let dzlic = std::fs::read_to_string(file).unwrap();
    assert!(dzlic.starts_with("# DZBuild POS licence"));
    assert_eq!(
        verify(&dzlic, &ring_of(&e), device.code()).unwrap().key,
        text,
        "the .dzlic holds the same key"
    );
    let out_file = dir.path().join("pour-le-client.dzlic");
    let lifetime = run_with(
        &mut e,
        &[
            "issue",
            "--device",
            device.code(),
            "--kind",
            "lifetime",
            "--out",
            out_file.to_str().unwrap(),
        ],
    )
    .unwrap();
    assert!(out_file.is_file() && lifetime.contains("lifetime"));
}

#[test]
fn a_second_trial_for_a_device_is_refused_and_days_are_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = keyed(dir.path());
    let code = testkit::device().code().to_owned();
    run_with(&mut e, &["issue", "--device", &code, "--kind", "trial"]).unwrap();
    let spaced = code.replace('-', " ");
    let refused = run_with(&mut e, &["issue", "--device", &spaced, "--kind", "trial"]).unwrap_err();
    assert!(refused.0.contains("one trial per PC"), "{refused}");
    assert_eq!(Ledger::load(&e.keys).unwrap().licences.len(), 1);
    let other = testkit::other_device().code().to_owned();
    let out = run_with(
        &mut e,
        &[
            "issue", "--device", &other, "--kind", "trial", "--days", "14",
        ],
    )
    .unwrap();
    let verified = verify(&key_in(&out), &ring_of(&e), &other).unwrap();
    assert_eq!(verified.payload.expires_at, Some(e.now + 14 * DAY_MS));
}

#[test]
fn lifetime_rules_and_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = keyed(dir.path());
    let a = testkit::device().code().to_owned();
    let b = testkit::other_device().code().to_owned();
    run_with(&mut e, &["issue", "--device", &a, "--kind", "lifetime"]).unwrap();
    let first = Ledger::load(&e.keys).unwrap().licences[0].id.clone();
    let twice = run_with(&mut e, &["issue", "--device", &a, "--kind", "lifetime"]).unwrap_err();
    assert!(
        twice.0.contains("already has the lifetime licence"),
        "{twice}"
    );
    let same_pc = run_with(
        &mut e,
        &[
            "issue",
            "--device",
            &a,
            "--kind",
            "lifetime",
            "--replace",
            &first,
        ],
    )
    .unwrap_err();
    assert!(same_pc.0.contains("new PC"), "{same_pc}");
    let moved = run_with(
        &mut e,
        &[
            "issue",
            "--device",
            &b,
            "--kind",
            "lifetime",
            "--replace",
            &first,
        ],
    )
    .unwrap();
    assert!(
        moved.contains("REVOKED") && moved.contains(&format!("\"{first}\",")),
        "{moved}"
    );
    let ledger = Ledger::load(&e.keys).unwrap();
    let second = ledger.licences[1].id.clone();
    assert_eq!(
        ledger.get(&first).and_then(|l| l.replaced_by.clone()),
        Some(second.clone())
    );
    assert_eq!(
        ledger.get(&second).and_then(|l| l.replaces.clone()),
        Some(first.clone())
    );
    let c = third_device().code().to_owned();
    let stale = run_with(
        &mut e,
        &[
            "issue",
            "--device",
            &c,
            "--kind",
            "lifetime",
            "--replace",
            &first,
        ],
    )
    .unwrap_err();
    assert!(stale.0.contains("not a live lifetime licence"), "{stale}");
    let trial_moved = run_with(
        &mut e,
        &[
            "issue",
            "--device",
            &c,
            "--kind",
            "trial",
            "--replace",
            &second,
        ],
    )
    .unwrap_err();
    assert!(trial_moved.0.contains("never moved"), "{trial_moved}");
}

#[test]
fn inspect_list_and_revoke() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = keyed(dir.path());
    let a = testkit::device().code().to_owned();
    let text = key_in(
        &run_with(
            &mut e,
            &[
                "issue",
                "--device",
                &a,
                "--kind",
                "lifetime",
                "--customer",
                "Kiosque Nour",
            ],
        )
        .unwrap(),
    );
    let shown = run_with(&mut e, &["inspect", &text]).unwrap();
    assert!(
        shown.contains("signature: valid (key k1)")
            && shown.contains("lifetime")
            && shown.contains(&a),
        "{shown}"
    );
    let ledger = Ledger::load(&e.keys).unwrap();
    let id = ledger.licences[0].id.clone();
    let file = e.keys.join(ISSUED_DIR).join(format!("{id}.dzlic"));
    assert!(
        run_with(&mut e, &["inspect", file.to_str().unwrap()])
            .unwrap()
            .contains("signature: valid")
    );
    // one symbol of the signature changed (not the last one: its trailing bits must stay zero)
    let mut symbols: Vec<char> = text.chars().collect();
    let at = symbols.len() - 10;
    symbols[at] = if symbols[at] == 'A' { 'B' } else { 'A' };
    let forged: String = symbols.into_iter().collect();
    assert!(
        run_with(&mut e, &["inspect", &forged])
            .unwrap()
            .contains("NOT VALID")
    );
    assert!(
        run_with(&mut e, &["inspect", "bonjour"])
            .unwrap_err()
            .0
            .contains("not a DZBuild POS licence key")
    );
    let listed = run_with(&mut e, &["list"]).unwrap();
    assert!(
        listed.contains(&id) && listed.contains("live") && listed.contains("Kiosque Nour"),
        "{listed}"
    );
    let one = run_with(&mut e, &["list", "--device", &a.to_lowercase()]).unwrap();
    assert!(one.contains(&text), "the key is printed again for one PC");
    let revoked = run_with(&mut e, &["revoke", &id]).unwrap();
    assert!(revoked.contains(&format!("\"{id}\",")), "{revoked}");
    assert!(run_with(&mut e, &["list"]).unwrap().contains("revoked"));
    assert!(
        run_with(&mut e, &["revoke", &id])
            .unwrap_err()
            .0
            .contains("already revoked")
    );
}

#[test]
fn bad_input_is_explained() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = keyed(dir.path());
    let device = testkit::device();
    let a = device.code();
    let help = run_with(&mut e, &[]).unwrap();
    assert!(help.contains("keygen") && help.contains("issue") && help.contains("--replace"));
    assert_eq!(run_with(&mut e, &["help"]).unwrap(), help);
    let cases: [(&[&str], &str); 7] = [
        (&["sign"], "unknown command"),
        (&["issue", "--colour", "red"], "has no --colour"),
        (
            &["issue", "--device", a, "--kind", "lifetime", "--days", "3"],
            "trial only",
        ),
        (
            &["issue", "--device", a, "--kind", "trial", "--days", "40"],
            "1 to 30",
        ),
        (
            &[
                "issue",
                "--device",
                "DZPOS-UUUU-UUUU-UUUU-UUUU",
                "--kind",
                "trial",
            ],
            "not a device code",
        ),
        (&["issue", "--device", a], "--kind"),
        (&["inspect"], "takes 1 value"),
    ];
    for (args, expected) in cases {
        let err = run_with(&mut e, args).unwrap_err();
        assert!(err.0.contains(expected), "{args:?}: {err}");
    }
}

/// The check symbol stops a code copied with one wrong symbol or two neighbours swapped
/// before anything is signed or listed, and `inspect` names a key whose code fails it.
#[test]
fn a_mistyped_device_code_is_refused_and_inspect_names_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = keyed(dir.path());
    let good = testkit::device().code().to_owned();
    let symbols: Vec<char> = good["DZPOS-".len()..]
        .chars()
        .filter(|c| *c != '-')
        .collect();
    let other = |c: char| if c == 'X' { 'Y' } else { 'X' };
    let mut typo = symbols.clone();
    typo[5] = other(typo[5]);
    let mut swapped = symbols.clone();
    let at = (0..15).find(|i| swapped[*i] != swapped[i + 1]).unwrap();
    swapped.swap(at, at + 1);
    let mut wrong_check = symbols.clone();
    wrong_check[15] = other(wrong_check[15]);
    for wrong in [typo, swapped, wrong_check.clone()] {
        let wrong: String = wrong.into_iter().collect();
        for args in [
            &["issue", "--device", &wrong, "--kind", "trial"][..],
            &["list", "--device", &wrong][..],
        ] {
            let err = run_with(&mut e, args).unwrap_err();
            assert!(err.0.contains("not a device code"), "{args:?}: {err}");
        }
    }
    assert!(Ledger::load(&e.keys).unwrap().licences.is_empty());
    // A key this folder signed for a code whose check symbol is wrong (the tool never issues one).
    let (key_id, signing) = keyfile::load(&e.keys, PASS).unwrap();
    let mut payload = testkit::payload(LicenseKind::Lifetime, &testkit::device(), e.now);
    payload.key_id = key_id;
    payload.device_code = format!("{}{}", &good[..good.len() - 1], wrong_check[15]);
    let forged = testkit::sign_bytes(&signing, &payload.canonical_bytes().unwrap());
    let shown = run_with(&mut e, &["inspect", &forged]).unwrap();
    assert!(
        shown.contains("NOT a device code") && shown.contains("signature: NOT VALID"),
        "{shown}"
    );
    let issued =
        key_in(&run_with(&mut e, &["issue", "--device", &good, "--kind", "trial"]).unwrap());
    let shown = run_with(&mut e, &["inspect", &issued]).unwrap();
    assert!(
        !shown.contains("NOT a device code") && shown.contains("signature: valid"),
        "{shown}"
    );
}

#[cfg(debug_assertions)]
#[test]
fn a_debug_build_issues_development_licences_without_the_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = env(dir.path());
    e.pass.clear(); // no passphrase is asked
    let a = testkit::device().code().to_owned();
    let out = run_with(
        &mut e,
        &["issue", "--dev", "--device", &a, "--kind", "lifetime"],
    )
    .unwrap();
    verify(&key_in(&out), &Keyring::embedded(), &a).unwrap();
    assert!(!e.keys.exists(), "no key folder, no ledger");
}
