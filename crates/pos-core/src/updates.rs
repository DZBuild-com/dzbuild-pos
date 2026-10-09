//! What decides whether an update is offered, downloaded and installed (spec §13, PLT-010,
//! PLT-011): the channel endpoints, the extras DZBuild adds to the Tauri static manifest (notes
//! in three languages, a kill switch, a staged rollout, a minimum version, the setup's digest
//! and size), and what blocks an install (an open cash session). Pure: the shell's updater
//! service (Task 12) asks, the Tauri plugin downloads and checks the signature.

use crate::audit::{self, Actor};
use crate::db::{meta_get, meta_set};
use crate::error::AppError;
use crate::terminal::{Terminal, TerminalRole};
use rusqlite::{Connection, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use specta::Type;

/// The manifest entries a Windows NSIS setup is looked up under, in the plugin's order: the
/// 32-bit build's (`i686`, 1.0.0) or the 64-bit one's (`x86_64`).
pub const fn platform_keys(x86: bool) -> [&'static str; 2] {
    if x86 {
        ["windows-i686-nsis", "windows-i686"]
    } else {
        ["windows-x86_64-nsis", "windows-x86_64"]
    }
}

/// This build's entries: a 32-bit app never takes the 64-bit setup's digest or signature.
pub const PLATFORM_KEYS: [&str; 2] = platform_keys(cfg!(target_arch = "x86"));
pub const META_APP_VERSION: &str = "app_version";

/// The architecture a setup is built for, as the manifest's platform keys name it.
pub const fn arch_name(x86: bool) -> &'static str {
    if x86 { "i686" } else { "x86_64" }
}

/// This build's architecture.
pub const ARCH: &str = arch_name(cfg!(target_arch = "x86"));

/// The field a main PC adds to the manifest it mirrors for its tills: the architecture of the
/// setup it keeps next to it (its own build's), since it mirrors only that one setup.
pub const MIRROR_ARCH: &str = "mirror_arch";

/// `raw` marked as mirroring a setup built for `arch`.
pub fn mark_mirror(raw: &Value, arch: &str) -> Value {
    let mut marked = raw.clone();
    if let Some(map) = marked.as_object_mut() {
        map.insert(MIRROR_ARCH.to_owned(), Value::from(arch));
    }
    marked
}

/// The main PC's mirror holds a setup this `arch` can install. A mirror without the mark holds
/// a 64-bit setup: only 64-bit builds existed before 1.0.0. A till of another arch asks the
/// internet instead (its own setup is on dzbuild.com).
pub fn mirror_serves(raw: &Value, arch: &str) -> bool {
    raw.get(MIRROR_ARCH)
        .and_then(Value::as_str)
        .unwrap_or(arch_name(false))
        == arch
}

/// The update channel. Named `UpdateChannel` for the bindings (tauri-specta's runtime reserves
/// `Channel`, and it names exported types after the Rust type), `Channel` in Rust.
pub type Channel = UpdateChannel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    Stable,
    Beta,
}

impl Channel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
        }
    }

    pub fn parse(s: &str) -> Option<Channel> {
        [Channel::Stable, Channel::Beta]
            .into_iter()
            .find(|c| c.as_str() == s)
    }
}

/// The manifest URL of `channel`, from the one configured in `tauri.conf.json`
/// (`https://…/stable/latest.json`): the beta channel lives next to it under `/beta/`.
pub fn channel_endpoint(configured: &str, channel: Channel) -> Result<String, AppError> {
    let invalid = || AppError::validation("endpoint", "update_endpoint_invalid");
    if !configured.starts_with("https://") {
        return Err(invalid());
    }
    let index = configured.rfind("/stable/").ok_or_else(invalid)?;
    Ok(match channel {
        Channel::Stable => configured.to_owned(),
        Channel::Beta => format!(
            "{}/beta/{}",
            &configured[..index],
            &configured[index + "/stable/".len()..]
        ),
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Notes {
    pub ar: String,
    pub fr: String,
    pub en: String,
}

/// DZBuild's additions to the Tauri static manifest; the plugin ignores them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestExtras {
    pub notes: Option<Notes>,
    /// The kill switch: a pulled release is never offered.
    pub paused: bool,
    /// Staged rollout, 0–100 % of the PCs (by `rollout_bucket`).
    pub rollout_percent: u8,
    /// Older versions must install the full setup first (a migration path).
    pub minimum_version: Option<String>,
    pub sha256: Option<String>,
    pub size: Option<u64>,
}

impl Default for ManifestExtras {
    fn default() -> ManifestExtras {
        ManifestExtras {
            notes: None,
            paused: false,
            rollout_percent: 100,
            minimum_version: None,
            sha256: None,
            size: None,
        }
    }
}

impl ManifestExtras {
    pub fn from_raw(raw: &Value) -> ManifestExtras {
        ManifestExtras::from_raw_with(raw, PLATFORM_KEYS)
    }

    /// The extras with the setup's digest and size read from the entries `keys`.
    pub fn from_raw_with(raw: &Value, keys: [&str; 2]) -> ManifestExtras {
        let platform = keys
            .iter()
            .find_map(|key| raw.get("platforms").and_then(|p| p.get(*key)));
        ManifestExtras {
            notes: raw
                .get("notes_i18n")
                .and_then(|n| serde_json::from_value(n.clone()).ok()),
            paused: raw.get("paused").and_then(Value::as_bool).unwrap_or(false),
            rollout_percent: raw
                .get("rollout")
                .and_then(Value::as_u64)
                .map(|p| u8::try_from(p.min(100)).unwrap_or(100))
                .unwrap_or(100),
            minimum_version: raw
                .get("minimum_version")
                .and_then(Value::as_str)
                .map(str::to_owned),
            sha256: platform
                .and_then(|p| p.get("sha256"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            size: platform.and_then(|p| p.get("size")).and_then(Value::as_u64),
        }
    }
}

/// This PC's place (0–99) in the rollout of `version`: the first two bytes of
/// SHA-256(`install_id` + "|" + `version`), so a PC keeps its place and each release draws anew.
pub fn rollout_bucket(install_id: &str, version: &str) -> u8 {
    let digest = Sha256::digest(format!("{install_id}|{version}").as_bytes());
    let value = u16::from_be_bytes([digest[0], digest[1]]);
    u8::try_from(value % 100).unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Decision {
    Offer,
    Paused,
    NotYet,
    TooOld {
        minimum: String,
    },
    /// A till takes the version its main PC runs, never another one (M6 review): `version` is
    /// the main PC's, `None` when it does not answer. The main PC updates first.
    MainPcFirst {
        version: Option<String>,
    },
}

/// The kill switch and the minimum version, which no PC skips.
fn refused(extras: &ManifestExtras, current: &str) -> Option<Decision> {
    if extras.paused {
        return Some(Decision::Paused);
    }
    let minimum = extras.minimum_version.as_ref()?;
    let older = match (
        semver::Version::parse(current),
        semver::Version::parse(minimum),
    ) {
        (Ok(current), Ok(minimum)) => current < minimum,
        _ => false,
    };
    older.then(|| Decision::TooOld {
        minimum: minimum.clone(),
    })
}

/// Whether the release `offered` is proposed to this PC running `current`. `rollout_id` places
/// the PC in the staged rollout: the shop's id (its main PC's) for every PC of a shop network,
/// so they all come in the same day; the install id of a single PC.
pub fn decide(extras: &ManifestExtras, current: &str, offered: &str, rollout_id: &str) -> Decision {
    if let Some(refused) = refused(extras, current) {
        return refused;
    }
    if rollout_bucket(rollout_id, offered) >= extras.rollout_percent {
        return Decision::NotYet;
    }
    Decision::Offer
}

/// The same versions, `v` prefix or not (semver when both parse).
fn same_version(a: &str, b: &str) -> bool {
    match (
        semver::Version::parse(a.trim_start_matches('v')),
        semver::Version::parse(b.trim_start_matches('v')),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// A till's decision (M6 review): it runs the version of its main PC (`main_pc`, from the main
/// PC's health; `None` when it does not answer), so a schema-changing release never cuts it off
/// for days. Only the main PC's own version is offered, whatever the rollout; the kill switch and
/// the minimum version still hold.
pub fn decide_for_till(
    extras: &ManifestExtras,
    current: &str,
    offered: &str,
    main_pc: Option<&str>,
) -> Decision {
    if let Some(refused) = refused(extras, current) {
        return refused;
    }
    match main_pc {
        Some(version) if same_version(version, offered) => Decision::Offer,
        other => Decision::MainPcFirst {
            version: other.map(str::to_owned),
        },
    }
}

/// The downloaded setup against the manifest's size and SHA-256, on top of the plugin's
/// minisign check: a cut or altered download never reaches `install`.
pub fn check_artifact(bytes: &[u8], extras: &ManifestExtras) -> Result<(), AppError> {
    if extras.size.is_some_and(|size| size != bytes.len() as u64) {
        return Err(AppError::conflict("update_corrupt"));
    }
    if let Some(expected) = &extras.sha256 {
        let actual = hex::encode(Sha256::digest(bytes));
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(AppError::conflict("update_corrupt"));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InstallBlocker {
    #[serde(rename_all = "camelCase")]
    CashSessionOpen {
        terminal_code: String,
        user_name: String,
        opened_at: i64,
    },
}

/// Why an install must wait (PLT-011: never during a shift). A till in the `client` role waits
/// for its own drawer only; a standalone PC or the LAN host waits for every open drawer of the
/// database (the host's copy holds every till's sessions, M5).
pub fn install_blockers(
    conn: &Connection,
    terminal: &Terminal,
) -> Result<Vec<InstallBlocker>, AppError> {
    let only: Option<&str> = match terminal.role {
        TerminalRole::Client => Some(terminal.id.as_str()),
        _ => None,
    };
    let mut stmt = conn.prepare(
        "SELECT COALESCE(t.code, s.terminal_id), COALESCE(u.display_name, ''), s.opened_at
         FROM cash_sessions s
         LEFT JOIN terminals t ON t.id = s.terminal_id
         LEFT JOIN users u ON u.id = s.user_id
         WHERE s.status = 'open' AND (?1 IS NULL OR s.terminal_id = ?1)
         ORDER BY s.opened_at",
    )?;
    let rows = stmt
        .query_map([only], |r| {
            Ok(InstallBlocker::CashSessionOpen {
                terminal_code: r.get(0)?,
                user_name: r.get(1)?,
                opened_at: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct VersionChange {
    pub from: Option<String>,
    pub to: String,
}

/// Records the running version in `app_meta` and journals `app.updated { from, to }` when it
/// changed (the first start after an update, or the first start of a new install).
pub fn record_app_version(
    tx: &Transaction<'_>,
    actor: &Actor,
    version: &str,
) -> Result<Option<VersionChange>, AppError> {
    let before = meta_get(tx, META_APP_VERSION)?;
    if before.as_deref() == Some(version) {
        return Ok(None);
    }
    meta_set(tx, META_APP_VERSION, version)?;
    let change = VersionChange {
        from: before,
        to: version.to_owned(),
    };
    audit::append(
        tx,
        &actor.event(
            "app.updated",
            "app",
            Some(version),
            json!({ "from": change.from, "to": change.to }),
        ),
    )?;
    Ok(Some(change))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{NOON, shop};
    use serde_json::json;
    use sha2::{Digest, Sha256};

    const STABLE: &str = "https://dzbuild.com/pos/updates/stable/latest.json";

    #[test]
    fn endpoints_must_be_https_and_name_a_stable_folder() {
        assert_eq!(channel_endpoint(STABLE, Channel::Stable).unwrap(), STABLE);
        assert_eq!(
            channel_endpoint(STABLE, Channel::Beta).unwrap(),
            "https://dzbuild.com/pos/updates/beta/latest.json"
        );
        // Copies installed before 1.0.0 were configured on dzbuild.dev: any HTTPS host still works.
        assert_eq!(
            channel_endpoint(
                "https://dzbuild.dev/pos/updates/stable/latest.json",
                Channel::Beta
            )
            .unwrap(),
            "https://dzbuild.dev/pos/updates/beta/latest.json"
        );
        for bad in [
            "http://dzbuild.com/pos/updates/stable/latest.json",
            "https://dzbuild.com/pos/updates/latest.json",
            "ftp://x/stable/latest.json",
        ] {
            assert_eq!(
                channel_endpoint(bad, Channel::Stable).unwrap_err().code(),
                "update_endpoint_invalid",
                "{bad}"
            );
        }
        assert_eq!(Channel::parse("beta"), Some(Channel::Beta));
        assert_eq!(Channel::parse("nightly"), None);
    }

    fn manifest(extra: serde_json::Value) -> serde_json::Value {
        let mut base = json!({
            "version": "0.6.1",
            "notes": "Corrections",
            "pub_date": "2026-10-15T09:00:00Z",
            "platforms": {
                "windows-x86_64-nsis": { "url": "https://dzbuild.com/pos/download/0.6.1/x.exe", "signature": "s", "sha256": "ab", "size": 3 },
                "windows-x86_64": { "url": "https://dzbuild.com/pos/download/0.6.1/x.exe", "signature": "s" }
            }
        });
        if let (Some(map), Some(extra)) = (base.as_object_mut(), extra.as_object()) {
            map.extend(extra.clone());
        }
        base
    }

    #[test]
    fn a_32_bit_build_reads_its_own_setup_entry() {
        assert_eq!(
            platform_keys(false),
            ["windows-x86_64-nsis", "windows-x86_64"]
        );
        assert_eq!(platform_keys(true), ["windows-i686-nsis", "windows-i686"]);
        assert_eq!(PLATFORM_KEYS, platform_keys(cfg!(target_arch = "x86")));
        let raw = manifest(json!({}));
        let mut both = raw.clone();
        if let Some(platforms) = both.get_mut("platforms").and_then(|p| p.as_object_mut()) {
            platforms.insert(
                "windows-i686-nsis".into(),
                json!({ "url": "https://dzbuild.com/pos/download/0.6.1/y.exe", "signature": "t", "sha256": "cd", "size": 2 }),
            );
        }
        let x64 = ManifestExtras::from_raw_with(&both, platform_keys(false));
        assert_eq!((x64.sha256.as_deref(), x64.size), (Some("ab"), Some(3)));
        let x86 = ManifestExtras::from_raw_with(&both, platform_keys(true));
        assert_eq!((x86.sha256.as_deref(), x86.size), (Some("cd"), Some(2)));
        let none = ManifestExtras::from_raw_with(&raw, platform_keys(true));
        assert_eq!(
            (none.sha256, none.size),
            (None, None),
            "never the other arch's digest"
        );
    }

    #[test]
    fn a_till_takes_only_a_mirror_built_for_its_own_arch() {
        assert_eq!(arch_name(false), "x86_64");
        assert_eq!(arch_name(true), "i686");
        assert_eq!(ARCH, arch_name(cfg!(target_arch = "x86")));
        let raw = manifest(json!({}));
        // Only 64-bit builds existed before 1.0.0: a mirror without the mark is a 64-bit setup.
        assert!(mirror_serves(&raw, "x86_64"));
        assert!(!mirror_serves(&raw, "i686"));
        let x86 = mark_mirror(&raw, "i686");
        assert_eq!(x86.get(MIRROR_ARCH).and_then(Value::as_str), Some("i686"));
        assert!(mirror_serves(&x86, "i686"));
        assert!(!mirror_serves(&x86, "x86_64"));
        let x64 = mark_mirror(&raw, "x86_64");
        assert!(mirror_serves(&x64, "x86_64"));
        assert!(!mirror_serves(&x64, "i686"));
        assert_eq!(
            ManifestExtras::from_raw_with(&x64, platform_keys(false)),
            ManifestExtras::from_raw_with(&raw, platform_keys(false)),
            "the mark changes nothing else"
        );
    }

    #[test]
    fn the_extras_are_read_from_the_manifest() {
        let raw = manifest(json!({
            "notes_i18n": { "ar": "تصحيحات", "fr": "Corrections", "en": "Fixes" },
            "paused": false,
            "rollout": 250,
            "minimum_version": "0.5.0"
        }));
        let extras = ManifestExtras::from_raw(&raw);
        assert_eq!(
            extras.notes.as_ref().map(|n| n.ar.as_str()),
            Some("تصحيحات")
        );
        assert_eq!(extras.rollout_percent, 100, "capped");
        assert_eq!(extras.minimum_version.as_deref(), Some("0.5.0"));
        assert_eq!(
            (extras.sha256.as_deref(), extras.size),
            (Some("ab"), Some(3)),
            "from the nsis entry first"
        );
        let bare = ManifestExtras::from_raw(&json!({ "version": "1.0.0", "platforms": {} }));
        assert_eq!(
            (bare.paused, bare.rollout_percent, bare.notes.clone()),
            (false, 100, None)
        );
    }

    #[test]
    fn a_paused_release_or_one_outside_the_rollout_bucket_is_not_offered() {
        let paused = ManifestExtras::from_raw(&manifest(json!({ "paused": true })));
        assert_eq!(decide(&paused, "0.6.0", "0.6.1", "pc-1"), Decision::Paused);
        let everyone = ManifestExtras::from_raw(&manifest(json!({})));
        assert_eq!(decide(&everyone, "0.6.0", "0.6.1", "pc-1"), Decision::Offer);
        let nobody = ManifestExtras::from_raw(&manifest(json!({ "rollout": 0 })));
        assert_eq!(decide(&nobody, "0.6.0", "0.6.1", "pc-1"), Decision::NotYet);
        let too_old = ManifestExtras::from_raw(&manifest(json!({ "minimum_version": "0.6.0" })));
        assert_eq!(
            decide(&too_old, "0.5.9", "0.6.1", "pc-1"),
            Decision::TooOld {
                minimum: "0.6.0".into()
            }
        );
        // About half the PCs at 50 %, and a PC keeps its bucket for a version.
        let half = ManifestExtras::from_raw(&manifest(json!({ "rollout": 50 })));
        let offered = (0..1_000)
            .filter(|i| decide(&half, "0.6.0", "0.6.1", &format!("install-{i}")) == Decision::Offer)
            .count();
        assert!((400..=600).contains(&offered), "{offered}");
        assert_eq!(
            rollout_bucket("pc-7", "0.6.1"),
            rollout_bucket("pc-7", "0.6.1")
        );
        assert!(rollout_bucket("pc-7", "0.6.1") < 100);
    }

    /// M6 review: a till is offered the version its main PC runs, whatever its own rollout
    /// bucket, and never a newer one (nor anything while the main PC does not answer); the kill
    /// switch and the minimum version still hold.
    #[test]
    fn a_till_takes_only_the_version_its_main_pc_runs() {
        let nobody = ManifestExtras::from_raw(&manifest(json!({ "rollout": 0 })));
        assert_eq!(
            decide_for_till(&nobody, "0.6.0", "0.6.1", Some("0.6.1")),
            Decision::Offer,
            "the main PC already runs it: the rollout is the main PC's"
        );
        assert_eq!(
            decide_for_till(&nobody, "0.6.0", "v0.6.1", Some("0.6.1")),
            Decision::Offer
        );
        let everyone = ManifestExtras::from_raw(&manifest(json!({})));
        assert_eq!(
            decide_for_till(&everyone, "0.6.0", "0.6.1", Some("0.6.0")),
            Decision::MainPcFirst {
                version: Some("0.6.0".into())
            },
            "never ahead of its main PC"
        );
        assert_eq!(
            decide_for_till(&everyone, "0.6.0", "0.6.1", None),
            Decision::MainPcFirst { version: None }
        );
        let paused = ManifestExtras::from_raw(&manifest(json!({ "paused": true })));
        assert_eq!(
            decide_for_till(&paused, "0.6.0", "0.6.1", Some("0.6.1")),
            Decision::Paused
        );
        // Every PC of a shop network shares the shop's place in the rollout.
        let half = ManifestExtras::from_raw(&manifest(json!({ "rollout": 50 })));
        assert_eq!(
            decide(&half, "0.6.0", "0.6.1", "shop-1"),
            decide(&half, "0.6.0", "0.6.1", "shop-1")
        );
    }

    #[test]
    fn a_truncated_or_altered_artifact_fails_the_digest_check() {
        let bytes = b"the signed setup".to_vec();
        let extras = ManifestExtras {
            sha256: Some(hex::encode(Sha256::digest(&bytes)).to_uppercase()),
            size: Some(bytes.len() as u64),
            ..ManifestExtras::default()
        };
        check_artifact(&bytes, &extras).unwrap();
        assert_eq!(
            check_artifact(&bytes[..10], &extras).unwrap_err().code(),
            "update_corrupt"
        );
        let mut altered = bytes.clone();
        altered[0] ^= 1;
        assert_eq!(
            check_artifact(&altered, &extras).unwrap_err().code(),
            "update_corrupt"
        );
        check_artifact(&altered, &ManifestExtras::default()).unwrap(); // nothing to compare: the plugin's signature check stands alone
    }

    #[test]
    fn a_standalone_or_host_is_blocked_by_any_open_session_a_client_only_by_its_own() {
        let shop = shop();
        assert!(
            shop.db
                .read({
                    let t = shop.terminal.clone();
                    move |c| install_blockers(c, &t)
                })
                .unwrap()
                .is_empty()
        );
        shop.open_till(100_000);
        let t = shop.terminal.clone();
        let blockers = shop.db.read(move |c| install_blockers(c, &t)).unwrap();
        assert_eq!(
            blockers,
            [InstallBlocker::CashSessionOpen {
                terminal_code: "C1".into(),
                user_name: "Yacine".into(),
                opened_at: NOON
            }]
        );
        // The same database seen by another till in the client role: C1's drawer is not its own.
        let mut client = shop.terminal.clone();
        client.id = "00000000-0000-7000-8000-0000000000c2".into();
        client.code = "C2".into();
        client.role = crate::terminal::TerminalRole::Client;
        assert!(
            shop.db
                .read(move |c| install_blockers(c, &client))
                .unwrap()
                .is_empty()
        );
        // C1 itself in the client role: its own open drawer blocks the install (the client's only
        // guard against installing mid-shift).
        let mut own = shop.terminal.clone();
        own.role = crate::terminal::TerminalRole::Client;
        assert_eq!(
            shop.db.read(move |c| install_blockers(c, &own)).unwrap(),
            blockers,
            "a client waits for its own drawer"
        );
        let mut host = shop.terminal.clone();
        host.id = "00000000-0000-7000-8000-0000000000c9".into();
        host.role = crate::terminal::TerminalRole::Host;
        assert_eq!(
            shop.db
                .read(move |c| install_blockers(c, &host))
                .unwrap()
                .len(),
            1,
            "the host waits for every till"
        );
    }

    #[test]
    fn a_version_change_is_journaled_once() {
        let shop = shop();
        let actor = crate::audit::Actor::system(None);
        let first = shop
            .db
            .write({
                let actor = actor.clone();
                move |tx| record_app_version(tx, &actor, "0.6.0")
            })
            .unwrap();
        assert_eq!(
            first,
            Some(VersionChange {
                from: None,
                to: "0.6.0".into()
            })
        );
        let again = shop
            .db
            .write({
                let actor = actor.clone();
                move |tx| record_app_version(tx, &actor, "0.6.0")
            })
            .unwrap();
        assert_eq!(again, None);
        let next = shop
            .db
            .write(move |tx| record_app_version(tx, &actor, "0.6.1"))
            .unwrap();
        assert_eq!(
            next,
            Some(VersionChange {
                from: Some("0.6.0".into()),
                to: "0.6.1".into()
            })
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'app.updated'"),
            2
        );
    }
}
