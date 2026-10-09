//! The main PC's copy for a new till (spec §4.6): one consistent read of the live database
//! (`Db::snapshot_to`), the main PC's own tables emptied in the copy (tokens, the feed and its
//! readers, notices, the tills' journals, its held carts, backups list and import history with
//! its undo images, `app_meta`, its cloud link), the copy's LAN triggers dropped, then `VACUUM`
//! so nothing deleted survives in free pages. The copy's feed head is the `seq` the till
//! continues from; its journal tail is the till's anchor.

use super::capture;
use super::catalogue;
use super::changelog;
use super::devices;
use crate::audit::GENESIS_HASH;
use crate::db::{Db, integrity_ok, migrations};
use crate::error::AppError;
use rusqlite::config::DbConfig;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

/// The main PC's own tables, emptied in a till's copy (M7's cloud tables when present): every
/// `LocalOnly` table of the catalogue but `lan_local` (reset to a single PC) and the catalogue's
/// search index, and every `HostOnly` one (M8's registers, delivery rounds and zakat). Children come before their parents, as the copy enforces foreign keys
/// (`scrub_is_every_local_only_table_but_the_kept_ones`,
/// `scrub_empties_children_before_their_parents`).
pub const SCRUB: &[&str] = &[
    "changelog",
    "sync_cursors",
    "lan_commits",
    "lan_devices",
    "lan_inbox",
    "lan_journal",
    "lan_notices",
    "lan_outbox",
    "backups_log",
    "held_carts",
    "import_job_rows",
    "import_jobs",
    "app_meta",
    "cloud_links",
    "cloud_map",
    "sync_outbox",
    "cloud_stock",
    "cloud_publish",
    "cloud_inbox",
    "cloud_endpoints",
    "cloud_notices",
    "online_order_lines",
    "online_orders",
    "remote_customer_flags",
    "cloud_backups",
    "cloud_backup_keys",
    "controlled_register",
    "jeweller_register",
    "delivery_round_stops",
    "delivery_rounds",
    "zakat_statements",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFile {
    pub path: PathBuf,
    pub seq: i64,
    pub sha256: String,
    pub size: u64,
    pub schema_version: i64,
}

fn invalid() -> AppError {
    AppError::conflict("snapshot_invalid")
}

/// SHA-256 of a file as lowercase hex, read in 64 KiB blocks.
pub fn sha256_file(path: &Path) -> Result<String, AppError> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0_u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize().as_slice()))
}

/// The copy's feed head, its journal tail and its schema version.
type Sanitised = (i64, (i64, String), i64);

/// Empties the main PC's own tables in the copy at `tmp`, drops its LAN triggers, marks it
/// cleanly shut down as a single PC, vacuums and checks it. The connection's triggers are off,
/// so the append-only guards of the scrubbed tables do not fire.
fn sanitise(tmp: &Path, now: i64) -> Result<Sanitised, AppError> {
    let mut copy = Connection::open(tmp)?;
    copy.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)?;
    let seq = changelog::max_seq(&copy)?;
    let anchor: (i64, String) = copy
        .query_row(
            "SELECT seq, hash FROM audit_log ORDER BY seq DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .unwrap_or_else(|| (0, GENESIS_HASH.to_owned()));
    let tx = copy.transaction()?;
    for table in SCRUB {
        if catalogue::present(&tx, table)? {
            tx.execute_batch(&format!("DELETE FROM {}", catalogue::quote(table)))?;
        }
    }
    // A main PC that was a till keeps its outbox's sequence: a till's copy numbers its own
    // outbox from 1 (`devices::set_journal_anchor` forgets the old line's numbers).
    tx.execute("DELETE FROM sqlite_sequence WHERE name = 'lan_outbox'", [])?;
    capture::uninstall(&tx)?;
    tx.execute(
        "UPDATE lan_local SET role = 'standalone', host_id = NULL, host_name = NULL, host_address = NULL,
                applied_seq = 0, paired_at = NULL, last_contact_at = NULL, skew_ms = NULL,
                host_schema = NULL, updated_at = ?1",
        [now],
    )?;
    tx.execute(
        "INSERT INTO app_meta (key, value) VALUES ('clean_shutdown', '1')",
        [],
    )?;
    tx.commit()?;
    copy.execute_batch("VACUUM")?;
    if !integrity_ok(&copy)? {
        return Err(invalid());
    }
    let schema: i64 = copy.pragma_query_value(None, "user_version", |r| r.get(0))?;
    // Closed before the caller renames the file: Windows refuses to rename an open file.
    copy.close().map_err(|(_, e)| AppError::from(e))?;
    Ok((seq, anchor, schema))
}

/// Writes the sanitised copy of the live database for `for_terminal` into `dir`, records the
/// copy's journal tail as that till's anchor, and returns the file with the feed `seq` it
/// stands for.
pub fn export(db: &Db, dir: &Path, for_terminal: &str, now: i64) -> Result<SnapshotFile, AppError> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("snapshot-{for_terminal}.db"));
    let tmp = dir.join(format!("snapshot-{for_terminal}.db.tmp"));
    for stale in [&path, &tmp] {
        if stale.exists() {
            std::fs::remove_file(stale)?;
        }
    }
    db.snapshot_to(&tmp)?;
    let (seq, anchor, schema_version) = match sanitise(&tmp, now) {
        Ok(done) => done,
        Err(error) => {
            // Never leave an unsanitised copy of the main PC's database behind.
            let _ = std::fs::remove_file(&tmp);
            return Err(error);
        }
    };
    std::fs::rename(&tmp, &path)?;
    let terminal = for_terminal.to_owned();
    db.write(move |tx| devices::set_journal_anchor(tx, &terminal, anchor.0, &anchor.1))?;
    Ok(SnapshotFile {
        sha256: sha256_file(&path)?,
        size: std::fs::metadata(&path)?.len(),
        path,
        seq,
        schema_version,
    })
}

/// A downloaded copy is the one announced, sound, of this app's schema, and sanitised.
/// Returns its schema version.
pub fn verify_download(path: &Path, sha256: &str, seq: i64) -> Result<i64, AppError> {
    if sha256_file(path)? != sha256 {
        return Err(invalid());
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| invalid())?;
    let schema: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|_| invalid())?;
    if schema != migrations::LATEST_VERSION {
        return Err(AppError::conflict("lan_version_mismatch"));
    }
    if !integrity_ok(&conn).unwrap_or(false) {
        return Err(invalid());
    }
    let (feed_rows, role): (i64, String) = conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM changelog), (SELECT role FROM lan_local)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| invalid())?;
    if feed_rows != 0 || role != "standalone" || seq < 0 {
        return Err(invalid());
    }
    Ok(schema)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Db, DbOptions};
    use crate::money::Money;
    use crate::sync::capture::{CaptureSet, install, installed};
    use crate::sync::{feed, notices};
    use crate::testkit::{NOON, shop_in};
    use rusqlite::Connection;

    fn host(dir: &Path) -> (crate::testkit::Shop, String) {
        let db = Db::open(
            &dir.join("host.db"),
            DbOptions {
                readers: 1,
                backup_dir: Some(dir.join("b")),
            },
        )
        .unwrap();
        let shop = shop_in(
            db,
            crate::company::TaxRegime::Reel,
            crate::company::PriceMode::Ttc,
        );
        shop.product("Lait", Money(10_000), &shop.tva19);
        let c2 = "00000000-0000-7000-8000-0000000000c2".to_owned();
        let id = c2.clone();
        shop.db
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
                notices::raise(
                    tx,
                    notices::NoticeKind::Oversold,
                    None,
                    "p",
                    serde_json::json!({}),
                    0,
                )?;
                tx.execute(
                    "UPDATE lan_local SET role = 'host', host_id = ?1, updated_at = 0",
                    ["00000000-0000-7000-8000-00000000a001"],
                )?;
                tx.execute(
                    "INSERT INTO held_carts (id, terminal_id, user_id, label, payload, created_at)
                     VALUES ('00000000-0000-7000-8000-00000000cafe', 't', 'u', 'x', '{}', 0)",
                    [],
                )?;
                // An import still running on the main PC, with the undo image of a row it updated.
                tx.execute(
                    "INSERT INTO import_jobs (id, kind, file_name, policy, status, mapping, next_row, created_at)
                     VALUES ('00000000-0000-7000-8000-00000000f11e', 'products', 'articles.xlsx', 'upsert',
                             'running', '{}', 2, 0)",
                    [],
                )?;
                tx.execute(
                    "INSERT INTO import_job_rows (job_id, row_no, entity_id, action, before)
                     VALUES ('00000000-0000-7000-8000-00000000f11e', 1, 'p', 'updated', '{\"name_fr\":\"Lait\"}')",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        shop.product("Pain", Money(1_000), &shop.tva19); // captured: the feed has rows
        (shop, c2)
    }

    #[test]
    fn a_snapshot_is_a_sanitised_copy_with_its_seq() {
        let dir = tempfile::tempdir().unwrap();
        let (shop, c2) = host(dir.path());
        let head = shop.db.read(feed::head).unwrap();
        let imports: i64 = shop
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT (SELECT COUNT(*) FROM import_jobs) + (SELECT COUNT(*) FROM import_job_rows)",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(imports, 2, "the main PC has an import in progress");
        let file = export(&shop.db, &dir.path().join("snapshots"), &c2, NOON).unwrap();
        assert_eq!(file.seq, head);
        assert_eq!(file.sha256, sha256_file(&file.path).unwrap());
        assert_eq!(file.schema_version, crate::db::migrations::LATEST_VERSION);
        let copy = Connection::open(&file.path).unwrap();
        let count = |sql: &str| copy.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
        // Every table of the main PC's own (`LocalOnly`) is empty in the copy, but `lan_local`
        // (reset to a single PC), `app_meta` (the clean mark) and the catalogue's search index:
        // a table a later migration adds to the catalogue is covered without naming it here.
        let kept = |name: &str| {
            matches!(name, "lan_local" | "app_meta") || name.starts_with("products_fts")
        };
        let mut emptied = Vec::new();
        for table in catalogue::schema_tables(&copy).unwrap() {
            let class = catalogue::class_of(&table);
            if (class == Some(catalogue::SyncClass::LocalOnly) && !kept(&table))
                || class == Some(catalogue::SyncClass::HostOnly)
            {
                assert_eq!(
                    count(&format!(
                        "SELECT COUNT(*) FROM {}",
                        catalogue::quote(&table)
                    )),
                    0,
                    "{table}"
                );
                emptied.push(table);
            }
        }
        for table in [
            "changelog",
            "sync_cursors",
            "lan_devices",
            "lan_notices",
            "lan_commits",
            "held_carts",
            "backups_log",
            "import_jobs",
            "import_job_rows",
        ] {
            assert!(emptied.iter().any(|t| t == table), "{table} is checked");
        }
        assert_eq!(count("SELECT COUNT(*) FROM products"), 2);
        assert_eq!(
            count("SELECT COUNT(*) FROM lan_local WHERE role = 'standalone' AND host_id IS NULL"),
            1
        );
        assert!(
            installed(&copy).unwrap().is_empty(),
            "the generated LAN triggers are dropped"
        );
        assert_eq!(
            count(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name = 'lan_outbox_kept'"
            ),
            1,
            "the migration's own guards stay"
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM app_meta WHERE key <> 'clean_shutdown'"),
            0
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM app_meta WHERE key = 'clean_shutdown' AND value = '1'"),
            1
        );
        let (anchor_seq, anchor_hash): (i64, String) = shop
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT journal_anchor_seq, journal_anchor_hash FROM lan_devices",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?)
            })
            .unwrap();
        let tail: (i64, String) = copy
            .query_row(
                "SELECT seq, hash FROM audit_log ORDER BY seq DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (anchor_seq, anchor_hash),
            tail,
            "the till's journal continues from here"
        );
        assert_eq!(
            verify_download(&file.path, &file.sha256, file.seq).unwrap(),
            crate::db::migrations::LATEST_VERSION
        );
    }

    #[test]
    fn scrub_is_every_local_only_table_but_the_kept_ones() {
        // A till's copy keeps `lan_local` (reset to a single PC) and the catalogue's search index;
        // the main PC's `HostOnly` tables (M8) are emptied too.
        let expected: std::collections::BTreeSet<&str> = catalogue::TABLES
            .iter()
            .filter(|t| {
                (t.class == catalogue::SyncClass::LocalOnly
                    && !matches!(t.name, "lan_local" | "products_fts"))
                    || t.class == catalogue::SyncClass::HostOnly
            })
            .map(|t| t.name)
            .collect();
        let scrub: std::collections::BTreeSet<&str> = SCRUB.iter().copied().collect();
        assert_eq!(scrub, expected);
        assert_eq!(scrub.len(), SCRUB.len(), "no table twice");
    }

    #[test]
    fn scrub_empties_children_before_their_parents() {
        // The copy's connection enforces foreign keys: a parent emptied first fails the export.
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(
            &dir.path().join("pos.db"),
            DbOptions {
                readers: 1,
                backup_dir: Some(dir.path().join("b")),
            },
        )
        .unwrap();
        let links: Vec<(String, String)> = db
            .read(|c| {
                let mut out = Vec::new();
                for child in SCRUB {
                    if !catalogue::present(c, child)? {
                        continue;
                    }
                    let mut stmt =
                        c.prepare("SELECT \"table\" FROM pragma_foreign_key_list(?1)")?;
                    for parent in stmt.query_map([child], |r| r.get::<_, String>(0))? {
                        out.push(((*child).to_owned(), parent?));
                    }
                }
                Ok(out)
            })
            .unwrap();
        let at = |name: &str| SCRUB.iter().position(|t| *t == name);
        let mut checked = 0;
        for (child, parent) in &links {
            if let (Some(c), Some(p)) = (at(child), at(parent))
                && c != p
            {
                assert!(c < p, "{child} is emptied before {parent}");
                checked += 1;
            }
        }
        assert!(
            checked >= 3,
            "import rows, online order lines and cloud backups have their parents in SCRUB: {links:?}"
        );
    }

    #[test]
    fn a_download_is_checked_before_it_is_staged() {
        let dir = tempfile::tempdir().unwrap();
        let (shop, c2) = host(dir.path());
        let file = export(&shop.db, &dir.path().join("snapshots"), &c2, NOON).unwrap();
        assert_eq!(
            verify_download(&file.path, &"0".repeat(64), file.seq)
                .unwrap_err()
                .code(),
            "snapshot_invalid"
        );
        let copy = Connection::open(&file.path).unwrap();
        copy.pragma_update(
            None,
            "user_version",
            crate::db::migrations::LATEST_VERSION + 1,
        )
        .unwrap();
        drop(copy);
        let sha = sha256_file(&file.path).unwrap();
        assert_eq!(
            verify_download(&file.path, &sha, file.seq)
                .unwrap_err()
                .code(),
            "lan_version_mismatch"
        );
        std::fs::write(&file.path, b"not a database").unwrap();
        let sha = sha256_file(&file.path).unwrap();
        assert_eq!(
            verify_download(&file.path, &sha, file.seq)
                .unwrap_err()
                .code(),
            "snapshot_invalid"
        );
    }
}
