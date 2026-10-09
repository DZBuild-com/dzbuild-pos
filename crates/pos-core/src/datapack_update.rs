//! Signed regulatory data packs (PLT-024): the rates, brackets, ceilings, nisab and territorial
//! lists change by law; a new pack ships as `dz-datapack-<id>.json` with its `.sig`, signed with
//! the owner's data-pack key (minisign, the Tauri updater's format). The app verifies the
//! signature with the key compiled in, parses and validates the pack, and keeps it in its data
//! folder; at the next start the newest valid pack already in force wins over the bundled one.
//! An unsigned, altered or unreadable file is never used.

use crate::audit::{self, Actor};
use crate::datapack::{self, Pack};
use crate::error::AppError;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use minisign_verify::{PublicKey, Signature};
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::path::Path;

/// The public half of the owner's data-pack key (its private half never leaves the
/// release PC, kept like the updater key: docs/release/updater-key.md). Base64 of the minisign public key file, as `tauri signer`
/// writes it.
pub const DATAPACK_PUBLIC_KEY: &str = include_str!("../data/datapack.pub");

fn bad_signature() -> AppError {
    AppError::validation("signature", "datapack_signature_invalid")
}

/// The text of a base64 minisign file (`tauri signer` writes the key and the signature so).
fn text_of(b64: &str) -> Result<String, AppError> {
    let bytes = STANDARD.decode(b64.trim()).map_err(|_| bad_signature())?;
    String::from_utf8(bytes).map_err(|_| bad_signature())
}

/// The pack in `json` when `sig` is its minisign signature by `public_key` (prehashed, as
/// `tauri signer sign` makes it) and the pack is well formed and valid
/// (`datapack_file_invalid` otherwise). The signature is checked first: nothing of an unsigned
/// file is read.
pub fn verify(json: &str, sig: &str, public_key: &str) -> Result<Pack, AppError> {
    let key = PublicKey::decode(&text_of(public_key)?).map_err(|_| bad_signature())?;
    let signature = Signature::decode(&text_of(sig)?).map_err(|_| bad_signature())?;
    key.verify(json.as_bytes(), &signature, false)
        .map_err(|_| bad_signature())?;
    let invalid = |_| AppError::validation("file", "datapack_file_invalid");
    let pack = datapack::parse(json).map_err(invalid)?;
    pack.validate().map_err(invalid)?;
    Ok(pack)
}

/// A verified pack kept in the data folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPack {
    pub id: String,
    pub version: u32,
    pub valid_from: String,
    pub file_name: String,
}

/// A pack file left aside at start, with the code of the reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub file_name: String,
    pub code: String,
}

/// Verifies the pack and writes it with its signature into `dir` for the next start; refused
/// unless signed by `public_key` and newer than `current`, the pack in force
/// (`datapack_older`).
pub fn install(
    dir: &Path,
    json: &str,
    sig: &str,
    current: &Pack,
    public_key: &str,
) -> Result<InstalledPack, AppError> {
    let pack = verify(json, sig, public_key)?;
    if pack.version <= current.version {
        return Err(AppError::rejected(
            json!({ "current": current.version, "offered": pack.version }),
            "datapack_older",
        ));
    }
    let safe: String = pack
        .id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
        .collect();
    let file_name = format!("dz-datapack-{safe}.json");
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(&file_name), json)?;
    std::fs::write(dir.join(format!("{file_name}.sig")), sig)?;
    Ok(InstalledPack {
        id: pack.id,
        version: pack.version,
        valid_from: pack.valid_from,
        file_name,
    })
}

/// Journals an installed pack (`datapack.installed`).
pub fn record_installed(
    tx: &Transaction<'_>,
    actor: &Actor,
    p: &InstalledPack,
) -> Result<(), AppError> {
    audit::append(
        tx,
        &actor.event(
            "datapack.installed",
            "datapack",
            Some(&p.id),
            json!({ "version": p.version, "validFrom": p.valid_from, "fileName": p.file_name }),
        ),
    )
    .map(drop)
}

/// The `.json` files of `dir` by name, each checked: the verified packs and the files left aside.
fn scan(dir: &Path, public_key: &str) -> (Vec<(String, Pack)>, Vec<Skipped>) {
    let (mut packs, mut skipped) = (Vec::new(), Vec::new());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (packs, skipped);
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    names.sort();
    for name in names {
        let read = |p: &str| std::fs::read_to_string(dir.join(p)).ok();
        let (Some(json), Some(sig)) = (read(&name), read(&format!("{name}.sig"))) else {
            skipped.push(Skipped {
                file_name: name,
                code: "datapack_signature_invalid".into(),
            });
            continue;
        };
        match verify(&json, &sig, public_key) {
            Ok(pack) => packs.push((name, pack)),
            Err(e) => skipped.push(Skipped {
                file_name: name,
                code: e.code(),
            }),
        }
    }
    (packs, skipped)
}

/// The pack to run with: the highest version among the valid signed packs in `dir` whose
/// `validFrom` has come by `today` (`YYYY-MM-DD`), else `bundled`. Files that fail are listed,
/// never used.
pub fn load_best(dir: &Path, bundled: Pack, today: &str, public_key: &str) -> (Pack, Vec<Skipped>) {
    let (packs, skipped) = scan(dir, public_key);
    let best = packs
        .into_iter()
        .map(|(_, pack)| pack)
        .filter(|p| p.version > bundled.version && p.valid_from.as_str() <= today)
        .max_by_key(|p| p.version)
        .unwrap_or(bundled);
    (best, skipped)
}

/// What a main PC tells its tills about the pack it runs with (final review): its id and version,
/// and its signed files when it is a pack installed in the data folder (the bundled pack is the
/// same in every copy of a version: no files).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostPack {
    pub id: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
}

/// [`HostPack`] of `active` (the pack this PC runs with), its files read from `dir` when one of
/// them is that pack, signed by `public_key`.
pub fn host_pack(dir: &Path, active: &Pack, public_key: &str) -> HostPack {
    let (packs, _) = scan(dir, public_key);
    let files = packs
        .into_iter()
        .find(|(_, p)| p.id == active.id && p.version == active.version)
        .and_then(|(name, _)| {
            let json = std::fs::read_to_string(dir.join(&name)).ok()?;
            let sig = std::fs::read_to_string(dir.join(format!("{name}.sig"))).ok()?;
            Some((json, sig))
        });
    let (json, sig) = files.unzip();
    HostPack {
        id: active.id.clone(),
        version: active.version,
        json,
        sig,
    }
}

/// A till follows its main PC's pack (final review): a newer signed pack the main PC runs with
/// is verified with `public_key` and installed into `dir` for the next start (`Some`); an older
/// or equal one, or one without its files, installs nothing (`None`).
pub fn follow(
    dir: &Path,
    current: &Pack,
    offered: &HostPack,
    public_key: &str,
) -> Result<Option<InstalledPack>, AppError> {
    if offered.version <= current.version {
        return Ok(None);
    }
    let (Some(json), Some(sig)) = (&offered.json, &offered.sig) else {
        return Ok(None);
    };
    install(dir, json, sig, current, public_key).map(Some)
}

/// `app_meta` keys of a till's packs: the one it runs with, its main PC's, and the one it
/// installed from its main PC for the next start.
const ACTIVE_KEY: &str = "datapack.active";
const HOST_KEY: &str = "datapack.host";
const FETCHED_KEY: &str = "datapack.fetched";

/// At every start: the version of the pack this PC runs with (read by [`gap`]).
pub fn note_active(tx: &Transaction<'_>, version: u32) -> Result<(), AppError> {
    crate::db::meta_set(tx, ACTIVE_KEY, &version.to_string())
}

/// On a till: its main PC's version, and the version installed from it when one was.
pub fn note_host(tx: &Transaction<'_>, host: u32, fetched: Option<u32>) -> Result<(), AppError> {
    crate::db::meta_set(tx, HOST_KEY, &host.to_string())?;
    if let Some(version) = fetched {
        crate::db::meta_set(tx, FETCHED_KEY, &version.to_string())?;
    }
    Ok(())
}

/// Whether this till's pack differs from its main PC's (final review): `restart` (the main PC's
/// newer pack is installed here, in force at the next start), `behind` (the main PC runs a newer
/// pack this till could not take: install it here too) or `ahead` (this till runs a newer pack
/// than the main PC: install it there). `None` when they match or are not known yet.
pub fn gap(conn: &rusqlite::Connection) -> Result<Option<String>, AppError> {
    let read = |key: &str| -> Result<Option<u32>, AppError> {
        Ok(crate::db::meta_get(conn, key)?.and_then(|v| v.parse().ok()))
    };
    let (Some(active), Some(host)) = (read(ACTIVE_KEY)?, read(HOST_KEY)?) else {
        return Ok(None);
    };
    Ok(if host > active {
        Some(if read(FETCHED_KEY)?.is_some_and(|f| f >= host) {
            "restart".into()
        } else {
            "behind".into()
        })
    } else if host < active {
        Some("ahead".into())
    } else {
        None
    })
}

/// The verified packs of `dir`, newest version first (in force or still to come).
pub fn installed(dir: &Path, public_key: &str) -> Vec<InstalledPack> {
    let mut out: Vec<InstalledPack> = scan(dir, public_key)
        .0
        .into_iter()
        .map(|(file_name, p)| InstalledPack {
            id: p.id,
            version: p.version,
            valid_from: p.valid_from,
            file_name,
        })
        .collect();
    out.sort_by(|a, b| {
        b.version
            .cmp(&a.version)
            .then_with(|| a.file_name.cmp(&b.file_name))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/datapack");

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{FIXTURES}/{name}")).unwrap()
    }

    fn test_key() -> String {
        fixture("test-only.key.pub")
    }

    /// Final review: the main PC serves the signed files of the installed pack it runs with (none
    /// for the bundled one); a till installs a newer one for its next start, nothing otherwise,
    /// and says how its pack stands against the main PC's.
    #[test]
    fn a_till_follows_its_main_pcs_signed_pack_and_says_when_they_differ() {
        let (host_dir, till_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let bundled = crate::datapack::bundled().unwrap();
        let none = host_pack(host_dir.path(), &bundled, &test_key());
        assert_eq!(
            (none.version, none.json, none.sig),
            (bundled.version, None, None)
        );
        let (json, sig) = (
            fixture("dz-datapack-test-v2.json"),
            fixture("dz-datapack-test-v2.json.sig"),
        );
        install(host_dir.path(), &json, &sig, &bundled, &test_key()).unwrap();
        let running = verify(&json, &sig, &test_key()).unwrap();
        let offered = host_pack(host_dir.path(), &running, &test_key());
        assert_eq!(offered.version, 2);
        assert_eq!(offered.json.as_deref(), Some(json.as_str()));
        assert_eq!(
            follow(till_dir.path(), &bundled, &offered, DATAPACK_PUBLIC_KEY)
                .unwrap_err()
                .code(),
            "datapack_signature_invalid",
            "a till trusts only the app's key"
        );
        let installed = follow(till_dir.path(), &bundled, &offered, &test_key())
            .unwrap()
            .expect("the newer pack");
        assert_eq!(installed.version, 2);
        assert_eq!(super::installed(till_dir.path(), &test_key()).len(), 1);
        assert_eq!(
            follow(till_dir.path(), &running, &offered, &test_key()).unwrap(),
            None,
            "the same version installs nothing"
        );

        let shop = crate::testkit::shop();
        assert_eq!(shop.db.read(gap).unwrap(), None, "not known yet");
        for (active, host, fetched, expected) in [
            (1, 1, None, None),
            (1, 2, None, Some("behind")),
            (1, 2, Some(2), Some("restart")),
            (2, 1, None, Some("ahead")),
        ] {
            shop.db
                .write(move |tx| {
                    tx.execute("DELETE FROM app_meta WHERE key LIKE 'datapack.%'", [])?;
                    note_active(tx, active)?;
                    note_host(tx, host, fetched)
                })
                .unwrap();
            assert_eq!(
                shop.db.read(gap).unwrap().as_deref(),
                expected,
                "{active} {host} {fetched:?}"
            );
        }
    }

    #[test]
    fn a_signed_pack_installs_and_a_tampered_or_older_one_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let (json, sig) = (
            fixture("dz-datapack-test-v2.json"),
            fixture("dz-datapack-test-v2.json.sig"),
        );
        let bundled = crate::datapack::bundled().unwrap();
        let pack = verify(&json, &sig, &test_key()).unwrap();
        assert_eq!(
            (pack.version, pack.regulated.items[0].ceiling),
            (2, crate::money::Money(13_000))
        );
        let tampered = json.replace("13000", "12000");
        assert_eq!(
            verify(&tampered, &sig, &test_key()).unwrap_err().code(),
            "datapack_signature_invalid"
        );
        assert_eq!(
            verify(&json, &sig, DATAPACK_PUBLIC_KEY).unwrap_err().code(),
            "datapack_signature_invalid",
            "only the app's key is trusted in production"
        );
        let installed = install(dir.path(), &json, &sig, &bundled, &test_key()).unwrap();
        assert_eq!(
            (installed.version, installed.id.as_str()),
            (2, "dz-2026.2-test")
        );
        let newer = verify(&json, &sig, &test_key()).unwrap();
        assert_eq!(
            install(dir.path(), &json, &sig, &newer, &test_key())
                .unwrap_err()
                .code(),
            "datapack_older"
        );
        assert_eq!(
            verify("{}", &sig, &test_key()).unwrap_err().code(),
            "datapack_signature_invalid"
        );
        assert_eq!(
            super::installed(dir.path(), &test_key()),
            [installed],
            "the installed pack is listed"
        );
    }

    #[test]
    fn the_newest_valid_pack_wins_at_start() {
        let dir = tempfile::tempdir().unwrap();
        let bundled = crate::datapack::bundled().unwrap();
        let (json, sig) = (
            fixture("dz-datapack-test-v2.json"),
            fixture("dz-datapack-test-v2.json.sig"),
        );
        install(dir.path(), &json, &sig, &bundled, &test_key()).unwrap();
        std::fs::write(dir.path().join("dz-broken.json"), "{}").unwrap();
        std::fs::write(dir.path().join("dz-broken.json.sig"), &sig).unwrap();
        let (pack, skipped) = load_best(dir.path(), bundled.clone(), "2026-09-28", &test_key());
        assert_eq!(pack.version, 2);
        assert_eq!(
            skipped.iter().map(|s| s.code.as_str()).collect::<Vec<_>>(),
            ["datapack_signature_invalid"]
        );
        let (early, _) = load_best(dir.path(), bundled.clone(), "2026-08-31", &test_key());
        assert_eq!(
            early.version, bundled.version,
            "a pack is not used before its validFrom"
        );
        let (unsigned, skipped) = load_best(
            dir.path(),
            bundled.clone(),
            "2026-09-28",
            DATAPACK_PUBLIC_KEY,
        );
        assert_eq!(
            unsigned.version, bundled.version,
            "never a pack the app's key did not sign"
        );
        assert_eq!(skipped.len(), 2);
        let (none, skipped) = load_best(
            &dir.path().join("absent"),
            bundled.clone(),
            "2026-09-28",
            &test_key(),
        );
        assert_eq!((none.version, skipped.len()), (bundled.version, 0));
    }

    /// A file without a readable signature or key is refused as unsigned before its content is
    /// read; a file the right key signed that is not a valid pack is refused as a file, not as a
    /// signature (`datapack_file_invalid`): the owner signed a broken file.
    #[test]
    fn the_signature_is_checked_before_the_content() {
        assert_eq!(
            verify("{}", "not base64!", &test_key()).unwrap_err().code(),
            "datapack_signature_invalid"
        );
        assert_eq!(
            verify("{}", &fixture("dz-datapack-test-v2.json.sig"), "garbage")
                .unwrap_err()
                .code(),
            "datapack_signature_invalid"
        );
        let (broken, sig) = (
            fixture("dz-datapack-test-broken.json"),
            fixture("dz-datapack-test-broken.json.sig"),
        );
        assert_eq!(
            verify(&broken, &sig, &test_key()).unwrap_err().code(),
            "datapack_file_invalid"
        );
        let dir = tempfile::tempdir().unwrap();
        let bundled = crate::datapack::bundled().unwrap();
        assert_eq!(
            install(dir.path(), &broken, &sig, &bundled, &test_key())
                .unwrap_err()
                .code(),
            "datapack_file_invalid"
        );
        assert!(
            std::fs::read_dir(dir.path()).unwrap().next().is_none(),
            "nothing is written"
        );
    }

    /// The key compiled into the app is a minisign public key (the owner's, kept in
    /// `crates/pos-core/data/datapack.pub`) and not the test key.
    #[test]
    fn the_app_trusts_the_owners_key_not_the_test_key() {
        let app = PublicKey::decode(&text_of(DATAPACK_PUBLIC_KEY).unwrap()).unwrap();
        let test = PublicKey::decode(&text_of(&test_key()).unwrap()).unwrap();
        assert_ne!(app, test);
    }
}
