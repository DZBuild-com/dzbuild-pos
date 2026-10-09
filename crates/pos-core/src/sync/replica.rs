//! Becoming a till (spec §4.6): the downloaded copy is staged in `lan/`, installed at the next
//! start **before** the database opens (the PC's previous database is kept as a `pre_pairing`
//! backup, like M0's restore keeps the replaced one), then adopted: this PC becomes terminal
//! `terminal_id` of that main PC. Leaving the network archives the till's copy the same way.

use super::capture::{CaptureSet, install};
use super::changelog::{self, LAN_OUTBOX_CONSUMER};
use super::outbox;
use crate::audit::{self, Actor};
use crate::db::{BackupKind, Db, META_INSTALL_ID, backup_file_name, meta_set};
use crate::error::AppError;
use crate::ids::new_id;
use crate::setup::META_SETUP_COMPLETED_AT;
use crate::terminal::{self, META_TERMINAL_ID, Terminal};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};

pub const LAN_DIR: &str = "lan";
const REPLICA_DB: &str = "replica.db";
const REPLICA_JSON: &str = "replica.json";
const ADOPT_JSON: &str = "adopt.json";
const RESET_MARKER: &str = "reset.pending";
/// Where a join writes the main PC's copy while it downloads (`pos_server::client::join`).
pub const DOWNLOAD_PART: &str = "download.part";

/// What a till needs to adopt a staged copy: the main PC it belongs to and the till it becomes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingReplica {
    pub seq: i64,
    pub host_id: String,
    pub host_name: String,
    pub host_address: String,
    pub terminal_id: String,
    pub terminal_code: String,
    pub terminal_name: String,
    pub paired_at: i64,
}

fn lan_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(LAN_DIR)
}

fn write_json(path: &Path, value: &PendingReplica) -> Result<(), AppError> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn read_json(path: &Path) -> Result<Option<PendingReplica>, AppError> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&std::fs::read(path)?)?))
}

/// Moves a verified download into `lan/` with what the till needs to adopt it. The marks of an
/// earlier attempt go first, so a cut in the middle never pairs one of them with this copy.
pub fn stage(data_dir: &Path, downloaded: &Path, pending: &PendingReplica) -> Result<(), AppError> {
    let dir = lan_dir(data_dir);
    std::fs::create_dir_all(&dir)?;
    let target = dir.join(REPLICA_DB);
    for stale in [dir.join(REPLICA_JSON), dir.join(ADOPT_JSON), target.clone()] {
        if stale.exists() {
            std::fs::remove_file(&stale)?;
        }
    }
    if std::fs::rename(downloaded, &target).is_err() {
        std::fs::copy(downloaded, &target)?;
        std::fs::remove_file(downloaded)?;
    }
    write_json(&dir.join(REPLICA_JSON), pending)
}

/// Removes a staged copy and what came with it (`replica.json`, `replica.db`, a partial download):
/// a join that cannot keep the till's token must not install a copy at the next start.
pub fn unstage(data_dir: &Path) -> Result<(), AppError> {
    let dir = lan_dir(data_dir);
    for name in [REPLICA_JSON, REPLICA_DB, DOWNLOAD_PART] {
        let path = dir.join(name);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

/// A staged copy waits to be installed at the next start.
pub fn has_pending(data_dir: &Path) -> bool {
    lan_dir(data_dir).join(REPLICA_JSON).exists()
}

/// Folds the WAL of a closed database into its file, so it can be moved alone.
fn settle(db_path: &Path) -> Result<(), AppError> {
    let conn = Connection::open(db_path)?;
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    conn.close().map_err(|(_, e)| AppError::from(e))?;
    for suffix in ["-wal", "-shm"] {
        let side = PathBuf::from(format!("{}{suffix}", db_path.display()));
        if side.exists() {
            std::fs::remove_file(side)?;
        }
    }
    Ok(())
}

/// Moves the closed database at `db_path` into the backups folder as a `pre_pairing` copy.
fn archive(db_path: &Path, backups_dir: &Path, now: i64) -> Result<Option<PathBuf>, AppError> {
    if !db_path.exists() {
        return Ok(None);
    }
    settle(db_path)?;
    std::fs::create_dir_all(backups_dir)?;
    let target = backups_dir.join(backup_file_name(now, BackupKind::PrePairing));
    std::fs::rename(db_path, &target)?;
    Ok(Some(target))
}

/// Before `Db::open`: the staged copy becomes `pos.db`; the previous one is kept in backups.
/// `adopt.json` is written first and moving the copy into place is the commit point, so a power
/// cut at any step leaves a state the next start finishes or undoes:
/// - before the move, the PC's own database is still `pos.db` and the copy still staged: the
///   install runs again (`pending_adoption` ignores a mark next to a staged copy);
/// - after it, `replica.json` is left with the copy gone and `adopt.json` kept: the install is
///   done, only `replica.json` goes.
///
/// If the move fails, the PC's own database is put back and `adopt.json` goes.
pub fn install_pending(
    data_dir: &Path,
    db_path: &Path,
    backups_dir: &Path,
    now: i64,
) -> Result<Option<PendingReplica>, AppError> {
    let dir = lan_dir(data_dir);
    let (replica_json, adopt_json) = (dir.join(REPLICA_JSON), dir.join(ADOPT_JSON));
    let Some(pending) = read_json(&replica_json)? else {
        return Ok(None);
    };
    let staged = dir.join(REPLICA_DB);
    if !staged.exists() {
        if adopt_json.exists() {
            // A start cut after the move: the copy is `pos.db` and waits for its adoption.
            std::fs::remove_file(&replica_json)?;
            return Ok(Some(pending));
        }
        return Err(AppError::conflict("snapshot_invalid"));
    }
    write_json(&adopt_json, &pending)?;
    if let Err(error) = swap(&staged, db_path, backups_dir, now) {
        // The PC's own database stays: nothing to adopt. A mark that cannot be removed is
        // harmless, since the copy is still staged (`pending_adoption`).
        let _ = std::fs::remove_file(&adopt_json);
        return Err(error);
    }
    std::fs::remove_file(&replica_json)?;
    Ok(Some(pending))
}

/// Archives the PC's database and moves the staged copy into its place; if the move fails, the
/// PC's own database is put back.
fn swap(staged: &Path, db_path: &Path, backups_dir: &Path, now: i64) -> Result<(), AppError> {
    let archived = archive(db_path, backups_dir, now)?;
    if let Err(error) = std::fs::rename(staged, db_path) {
        if let Some(previous) = archived {
            std::fs::rename(previous, db_path)?;
        }
        return Err(error.into());
    }
    Ok(())
}

/// An installed copy not adopted yet (read after `Db::open`): `adopt.json` once the staged copy
/// has been moved into place. While the copy is still staged, `pos.db` is the PC's own database
/// and is never adopted.
pub fn pending_adoption(data_dir: &Path) -> Result<Option<PendingReplica>, AppError> {
    let dir = lan_dir(data_dir);
    if dir.join(REPLICA_DB).exists() {
        return Ok(None);
    }
    read_json(&dir.join(ADOPT_JSON))
}

/// After `Db::open` on an installed copy: this PC becomes the till of that main PC.
pub fn adopt(
    db: &Db,
    data_dir: &Path,
    pending: &PendingReplica,
    now: i64,
) -> Result<Terminal, AppError> {
    let p = pending.clone();
    db.write_replica(move |tx| {
        meta_set(tx, META_TERMINAL_ID, &p.terminal_id)?;
        meta_set(tx, META_INSTALL_ID, &new_id())?;
        meta_set(tx, META_SETUP_COMPLETED_AT, &now.to_string())?;
        tx.execute(
            "UPDATE lan_local SET role = 'client', host_id = ?1, host_name = ?2, host_address = ?3,
                    applied_seq = ?4, paired_at = ?5, last_contact_at = ?6, updated_at = ?6",
            params![
                p.host_id,
                p.host_name,
                p.host_address,
                p.seq,
                p.paired_at,
                now
            ],
        )?;
        changelog::register(tx, LAN_OUTBOX_CONSUMER, now)?;
        Ok(())
    })?;
    let terminal_id = pending.terminal_id.clone();
    db.write(move |tx| install(tx, &CaptureSet::Client { terminal_id }).map(drop))?;
    let p = pending.clone();
    db.write(move |tx| {
        audit::append(
            tx,
            &Actor::system(Some(p.terminal_id.clone())).event(
                "lan.paired",
                "terminal",
                Some(&p.terminal_id),
                json!({ "hostId": p.host_id, "host": p.host_name, "code": p.terminal_code, "seq": p.seq }),
            ),
        )?;
        // The hook is not set yet at this point of the start: queue the journal row here.
        outbox::enqueue_from_changelog(tx, &p.terminal_id, now)?;
        Ok(())
    })?;
    let adopt_file = lan_dir(data_dir).join(ADOPT_JSON);
    if adopt_file.exists() {
        std::fs::remove_file(adopt_file)?;
    }
    db.read(terminal::local)?
        .ok_or_else(|| AppError::Internal("the adopted terminal is missing".into()))
}

/// A till leaves the network: its copy is archived at the next start (`apply_pending_reset`).
pub fn schedule_reset(data_dir: &Path) -> Result<(), AppError> {
    let dir = lan_dir(data_dir);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(RESET_MARKER), b"leave")?;
    Ok(())
}

/// Before `Db::open`: a till that left the network starts as a new PC; its copy is archived.
/// `true` when it archived.
pub fn apply_pending_reset(
    data_dir: &Path,
    db_path: &Path,
    backups_dir: &Path,
    now: i64,
) -> Result<bool, AppError> {
    let marker = lan_dir(data_dir).join(RESET_MARKER);
    if !marker.exists() {
        return Ok(false);
    }
    archive(db_path, backups_dir, now)?;
    std::fs::remove_file(marker)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Db, DbOptions};
    use crate::money::Money;
    use crate::sync::capture::{CaptureSet, install};
    use crate::sync::{feed, outbox, snapshot};
    use crate::testkit::{NOON, shop_in};

    fn opts(dir: &Path) -> DbOptions {
        DbOptions {
            readers: 1,
            backup_dir: Some(dir.join("backups")),
        }
    }

    /// A main PC with one product and C2 registered, and a new PC that was used alone before.
    fn setting(dir: &Path) -> (crate::testkit::Shop, PendingReplica, std::path::PathBuf) {
        let host_dir = dir.join("host");
        std::fs::create_dir_all(&host_dir).unwrap();
        let host = shop_in(
            Db::open(&host_dir.join("pos.db"), opts(&host_dir)).unwrap(),
            crate::company::TaxRegime::Reel,
            crate::company::PriceMode::Ttc,
        );
        host.product("Lait", Money(10_000), &host.tva19);
        let c2 = crate::ids::new_id();
        let id = c2.clone();
        host.db
            .write(move |tx| {
                install(tx, &CaptureSet::Host)?;
                tx.execute(
                    "INSERT INTO terminals (id, code, name, role, created_at) VALUES (?1, 'C2', 'Caisse 2', 'client', 0)",
                    [&id],
                )?;
                tx.execute(
                    "INSERT INTO lan_devices (terminal_id, name, token_hash, paired_at) VALUES (?1, 'Caisse 2', ?2, 0)",
                    rusqlite::params![id, "d".repeat(64)],
                )?;
                feed::register_till(tx, &id, 0)?;
                Ok(())
            })
            .unwrap();
        let file = snapshot::export(&host.db, &host_dir.join("snapshots"), &c2, NOON).unwrap();
        let till_dir = dir.join("till");
        std::fs::create_dir_all(&till_dir).unwrap();
        {
            // The new PC ran alone for a day: it has a shop, a product and a ticket of its own.
            let alone = shop_in(
                Db::open(&till_dir.join("pos.db"), opts(&till_dir)).unwrap(),
                crate::company::TaxRegime::Reel,
                crate::company::PriceMode::Ttc,
            );
            alone.product("Ancien article", Money(500), &alone.tva19);
            alone.db.clone().close().unwrap();
        }
        let pending = PendingReplica {
            seq: file.seq,
            host_id: "00000000-0000-7000-8000-00000000a001".into(),
            host_name: "Supérette El Amel".into(),
            host_address: "192.168.1.10:7345".into(),
            terminal_id: c2,
            terminal_code: "C2".into(),
            terminal_name: "Caisse 2".into(),
            paired_at: NOON,
        };
        stage(&till_dir, &file.path, &pending).unwrap();
        (host, pending, till_dir)
    }

    #[test]
    fn install_keeps_the_previous_database_and_adopt_makes_a_till() {
        let dir = tempfile::tempdir().unwrap();
        let (_host, pending, till_dir) = setting(dir.path());
        assert!(has_pending(&till_dir));
        let db_path = till_dir.join("pos.db");
        let installed =
            install_pending(&till_dir, &db_path, &till_dir.join("backups"), NOON).unwrap();
        assert_eq!(installed.as_ref(), Some(&pending));
        assert!(!has_pending(&till_dir));
        let kept: Vec<String> = std::fs::read_dir(till_dir.join("backups"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with("-pre_pairing.db"))
            .collect();
        assert_eq!(kept.len(), 1, "the PC's previous data is kept");
        let old = rusqlite::Connection::open(till_dir.join("backups").join(&kept[0])).unwrap();
        let n: i64 = old
            .query_row(
                "SELECT COUNT(*) FROM products WHERE name_fr = 'Ancien article'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);

        let db = Db::open(&db_path, opts(&till_dir)).unwrap();
        assert!(
            !db.report().unclean_shutdown,
            "the copy carries a clean mark"
        );
        let waiting = pending_adoption(&till_dir).unwrap().unwrap();
        let terminal = adopt(&db, &till_dir, &waiting, NOON).unwrap();
        assert_eq!(
            (terminal.id.as_str(), terminal.code.as_str()),
            (pending.terminal_id.as_str(), "C2")
        );
        assert!(pending_adoption(&till_dir).unwrap().is_none());
        let role: (String, i64) = db
            .read(|c| {
                Ok(
                    c.query_row("SELECT role, applied_seq FROM lan_local", [], |r| {
                        Ok((r.get(0)?, r.get(1)?))
                    })?,
                )
            })
            .unwrap();
        assert_eq!(role, ("client".into(), pending.seq));
        assert!(db.read(crate::setup::status).unwrap().completed);
        let items = db.read(|c| outbox::pending(c, 10)).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].bundle.kind, "lan.paired");
        let products: i64 = db
            .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM products", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(products, 1, "the main PC's catalogue, not the old one");
        let refused = db
            .write(|tx| Ok(tx.execute("UPDATE products SET sell_price = 1", [])?))
            .unwrap_err();
        assert_eq!(refused.code(), "host_required", "a till's guards are on");
    }

    #[test]
    fn a_failed_install_leaves_the_current_database() {
        let dir = tempfile::tempdir().unwrap();
        let (_host, _pending, till_dir) = setting(dir.path());
        std::fs::remove_file(till_dir.join(LAN_DIR).join("replica.db")).unwrap();
        let db_path = till_dir.join("pos.db");
        assert!(install_pending(&till_dir, &db_path, &till_dir.join("backups"), NOON).is_err());
        let db = Db::open(&db_path, opts(&till_dir)).unwrap();
        let n: i64 = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM products WHERE name_fr = 'Ancien article'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(n, 1, "still the PC's own database");
    }

    fn pre_pairing_backups(till_dir: &Path) -> usize {
        std::fs::read_dir(till_dir.join("backups"))
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with("-pre_pairing.db")
            })
            .count()
    }

    #[test]
    fn a_start_cut_after_the_swap_finds_the_copy_installed() {
        let dir = tempfile::tempdir().unwrap();
        let (_host, pending, till_dir) = setting(dir.path());
        let db_path = till_dir.join("pos.db");
        let backups = till_dir.join("backups");
        install_pending(&till_dir, &db_path, &backups, NOON).unwrap();
        // The power went off once the copy was in place, before `replica.json` was removed.
        write_json(&till_dir.join(LAN_DIR).join(REPLICA_JSON), &pending).unwrap();
        assert!(has_pending(&till_dir));
        let installed = install_pending(&till_dir, &db_path, &backups, NOON + 1).unwrap();
        assert_eq!(installed.as_ref(), Some(&pending));
        assert!(!has_pending(&till_dir));
        assert_eq!(
            pre_pairing_backups(&till_dir),
            1,
            "the copy is not archived as if it were the PC's own database"
        );
        let db = Db::open(&db_path, opts(&till_dir)).unwrap();
        let waiting = pending_adoption(&till_dir).unwrap().unwrap();
        assert_eq!(waiting, pending);
        let terminal = adopt(&db, &till_dir, &waiting, NOON + 1).unwrap();
        assert_eq!(terminal.code, "C2");
        let products: i64 = db
            .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM products", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(products, 1, "the main PC's catalogue");
    }

    #[test]
    fn a_start_cut_before_the_swap_installs_again() {
        let dir = tempfile::tempdir().unwrap();
        let (_host, pending, till_dir) = setting(dir.path());
        // The power went off after `adopt.json` was written, before the copy was moved.
        write_json(&till_dir.join(LAN_DIR).join(ADOPT_JSON), &pending).unwrap();
        assert!(
            pending_adoption(&till_dir).unwrap().is_none(),
            "the PC's own database is never adopted"
        );
        let db_path = till_dir.join("pos.db");
        let installed =
            install_pending(&till_dir, &db_path, &till_dir.join("backups"), NOON).unwrap();
        assert_eq!(installed.as_ref(), Some(&pending));
        assert_eq!(
            pending_adoption(&till_dir).unwrap().as_ref(),
            Some(&pending)
        );
        assert_eq!(pre_pairing_backups(&till_dir), 1);
    }

    #[cfg(windows)]
    #[test]
    fn a_failed_swap_puts_the_pcs_database_back_with_nothing_to_adopt() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let (_host, _pending, till_dir) = setting(dir.path());
        let db_path = till_dir.join("pos.db");
        // Another program holds the staged copy open without sharing: it cannot be moved.
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(till_dir.join(LAN_DIR).join(REPLICA_DB))
            .unwrap();
        let error =
            install_pending(&till_dir, &db_path, &till_dir.join("backups"), NOON).unwrap_err();
        assert!(
            matches!(error, AppError::Io(_)),
            "the move failed: {error:?}"
        );
        drop(lock);
        assert!(!till_dir.join(LAN_DIR).join(ADOPT_JSON).exists());
        assert!(pending_adoption(&till_dir).unwrap().is_none());
        assert!(has_pending(&till_dir), "the next start tries again");
        assert_eq!(pre_pairing_backups(&till_dir), 0, "put back, not archived");
        let db = Db::open(&db_path, opts(&till_dir)).unwrap();
        let n: i64 = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM products WHERE name_fr = 'Ancien article'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(n, 1, "still the PC's own database");
    }

    #[test]
    fn staging_again_forgets_an_old_adoption_mark() {
        let dir = tempfile::tempdir().unwrap();
        let (host, pending, till_dir) = setting(dir.path());
        write_json(&till_dir.join(LAN_DIR).join(ADOPT_JSON), &pending).unwrap();
        let file = snapshot::export(
            &host.db,
            &dir.path().join("host").join("snapshots"),
            &pending.terminal_id,
            NOON + 1,
        )
        .unwrap();
        stage(&till_dir, &file.path, &pending).unwrap();
        assert!(!till_dir.join(LAN_DIR).join(ADOPT_JSON).exists());
        assert!(has_pending(&till_dir));
    }

    #[test]
    fn leaving_archives_the_tills_copy_at_the_next_start() {
        let dir = tempfile::tempdir().unwrap();
        let (_host, _pending, till_dir) = setting(dir.path());
        let db_path = till_dir.join("pos.db");
        install_pending(&till_dir, &db_path, &till_dir.join("backups"), NOON).unwrap();
        schedule_reset(&till_dir).unwrap();
        assert!(
            apply_pending_reset(&till_dir, &db_path, &till_dir.join("backups"), NOON + 1).unwrap()
        );
        assert!(!db_path.exists(), "the next start opens a fresh database");
        let archived = std::fs::read_dir(till_dir.join("backups")).unwrap().count();
        assert_eq!(archived, 2, "the old PC database and the till's copy");
        assert!(
            !apply_pending_reset(&till_dir, &db_path, &till_dir.join("backups"), NOON + 2).unwrap()
        );
    }
}
