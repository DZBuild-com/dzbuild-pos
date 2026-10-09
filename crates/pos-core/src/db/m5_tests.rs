//! Migration 0007 (LAN): the change feed, this PC's LAN state, the till's outbox, the host's
//! devices, inbox, tills' journals, commit marks and notices, their guards, and the upgrade of
//! the previous release's database with nothing replicated. The change feed (`changelog`,
//! `sync_cursors`) is M7's, from 0006; on this branch `LIST` holds 0001-0004, 0006 and 0007
//! (M4's 0005 is ordered before 0006 at the merge), so the schema version here is 6.

use super::*;
use crate::audit;
use crate::documents;
use std::path::PathBuf;

fn count(db: &Db, sql: &'static str) -> i64 {
    db.read(move |c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .unwrap()
}

fn exec(db: &Db, sql: &'static str) -> Result<(), AppError> {
    db.write(move |tx| {
        tx.execute_batch(sql)?;
        Ok(())
    })
}

fn table_exists(db: &Db, name: &'static str) -> bool {
    db.read(move |c| {
        Ok(c.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |r| r.get::<_, i64>(0),
        )?)
    })
    .unwrap()
        == 1
}

const TERMINAL: &str = "INSERT INTO terminals (id, code, name, role, created_at)
    VALUES ('00000000-0000-7000-8000-0000000000c2', 'C2', 'Caisse 2', 'client', 0);";

const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn the_lan_tables_exist_and_this_pc_starts_standalone() {
    let db = Db::open_memory().unwrap();
    assert_eq!(db.report().schema_version, 9);
    for name in [
        "changelog",
        "sync_cursors",
        "lan_local",
        "lan_outbox",
        "lan_devices",
        "lan_inbox",
        "lan_journal",
        "lan_commits",
        "lan_notices",
    ] {
        assert!(table_exists(&db, name), "{name}");
    }
    assert_eq!(count(&db, "SELECT COUNT(*) FROM lan_local"), 1);
    assert_eq!(
        db.read(|c| Ok(c.query_row("SELECT role FROM lan_local", [], |r| r.get::<_, String>(0))?))
            .unwrap(),
        "standalone"
    );
    assert!(
        exec(&db, "DELETE FROM lan_local").is_err(),
        "the row is kept"
    );
    assert!(
        exec(&db, "UPDATE lan_local SET role = 'client'").is_err(),
        "a till names its host"
    );
    exec(
        &db,
        "UPDATE lan_local SET role = 'client', host_id = '00000000-0000-7000-8000-00000000a001',
                host_address = '192.168.1.10:7345', paired_at = 1, updated_at = 1",
    )
    .unwrap();
    assert!(
        exec(
            &db,
            "INSERT INTO lan_local (id, role, updated_at) VALUES (2, 'host', 0)"
        )
        .is_err(),
        "one row"
    );
}

#[test]
fn outbox_items_are_frozen_ordered_and_kept_until_sent() {
    let db = Db::open_memory().unwrap();
    let insert = |id: &'static str, key: &'static str| {
        let sql: &'static str = Box::leak(
            format!(
                "INSERT INTO lan_outbox (id, idempotency_key, terminal_id, kind, payload, rows, created_at)
                 VALUES ('{id}', '{key}', '00000000-0000-7000-8000-0000000000c2', 'sale.created',
                         '{{\"rows\":[],\"audit\":[]}}', 0, 1)"
            )
            .into_boxed_str(),
        );
        exec(&db, sql)
    };
    insert(
        "00000000-0000-7000-8000-00000000b001",
        "sale:00000000-0000-7000-8000-00000000f001",
    )
    .unwrap();
    insert(
        "00000000-0000-7000-8000-00000000b002",
        "tx:00000000-0000-7000-8000-00000000f002",
    )
    .unwrap();
    assert_eq!(
        count(
            &db,
            "SELECT MAX(local_seq) - MIN(local_seq) FROM lan_outbox"
        ),
        1,
        "local order"
    );
    assert!(
        insert(
            "00000000-0000-7000-8000-00000000b003",
            "sale:00000000-0000-7000-8000-00000000f001"
        )
        .is_err(),
        "a key is used once"
    );
    assert!(insert("00000000-0000-7000-8000-00000000b004", "sale with space").is_err());
    assert!(
        exec(&db, "UPDATE lan_outbox SET payload = '{}'").is_err(),
        "payload frozen"
    );
    assert!(
        exec(&db, "UPDATE lan_outbox SET idempotency_key = 'x'").is_err(),
        "key frozen"
    );
    exec(
        &db,
        "UPDATE lan_outbox SET attempts = attempts + 1, last_error = 'lan_offline'",
    )
    .unwrap();
    assert!(
        exec(&db, "DELETE FROM lan_outbox").is_err(),
        "pending items are kept"
    );
    assert!(
        exec(&db, "UPDATE lan_outbox SET status = 'sent'").is_err(),
        "sent says when and to which host"
    );
    exec(
        &db,
        "UPDATE lan_outbox SET status = 'sent', sent_at = 5, host_id = '00000000-0000-7000-8000-00000000a001'
         WHERE local_seq = (SELECT MIN(local_seq) FROM lan_outbox)",
    )
    .unwrap();
    assert!(
        exec(
            &db,
            "UPDATE lan_outbox SET status = 'pending' WHERE status = 'sent'"
        )
        .is_err(),
        "sent is final"
    );
    exec(&db, "DELETE FROM lan_outbox WHERE status = 'sent'").unwrap();
    exec(
        &db,
        "UPDATE lan_outbox SET status = 'rejected', last_error = 'replay_forbidden_row'",
    )
    .unwrap();
    assert!(
        exec(&db, "DELETE FROM lan_outbox").is_err(),
        "rejected items are kept"
    );
}

#[test]
fn devices_keep_hashed_tokens_and_are_never_deleted() {
    let db = Db::open_memory().unwrap();
    exec(&db, TERMINAL).unwrap();
    let device = "INSERT INTO lan_devices (terminal_id, name, token_hash, paired_at)
        VALUES ('00000000-0000-7000-8000-0000000000c2', 'Caisse 2',
                'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 1)";
    exec(&db, device).unwrap();
    assert!(
        exec(
            &db,
            "INSERT INTO lan_devices (terminal_id, name, token_hash, paired_at)
             VALUES ('00000000-0000-7000-8000-0000000000c9', 'X', 'short', 1)"
        )
        .is_err(),
        "a token hash is 64 hex characters, and the terminal exists"
    );
    assert!(exec(&db, "DELETE FROM lan_devices").is_err());
    assert!(
        exec(&db, "UPDATE lan_devices SET revoked_at = 5").is_err(),
        "a revocation says who"
    );
    exec(
        &db,
        "UPDATE lan_devices SET revoked_at = 5, revoked_by = 'u1'",
    )
    .unwrap();
}

#[test]
fn the_inbox_and_the_tills_journals_are_append_only() {
    let db = Db::open_memory().unwrap();
    exec(
        &db,
        "INSERT INTO lan_inbox (idempotency_key, terminal_id, local_seq, kind, client_created_at,
                                received_at, result, host_seq)
         VALUES ('sale:00000000-0000-7000-8000-00000000f001', '00000000-0000-7000-8000-0000000000c2', 1,
                 'sale.created', 1, 2, '{\"status\":\"applied\"}', 10)",
    )
    .unwrap();
    assert!(exec(&db, "UPDATE lan_inbox SET received_at = 3").is_err());
    assert!(exec(&db, "DELETE FROM lan_inbox").is_err());
    let row = format!(
        "INSERT INTO lan_journal (terminal_id, seq, id, at, action, entity_type, payload, prev_hash, hash,
                                  intact, received_at, merge_prev, merge_hash)
         VALUES ('00000000-0000-7000-8000-0000000000c2', 1, '00000000-0000-7000-8000-00000000e001', 1,
                 'sale.created', 'document', '{{}}', '{HASH_A}', '{HASH_A}', 1, 2, '{HASH_A}', '{HASH_A}')"
    );
    db.write(move |tx| Ok(tx.execute_batch(&row)?)).unwrap();
    assert!(exec(&db, "UPDATE lan_journal SET intact = 0").is_err());
    assert!(exec(&db, "DELETE FROM lan_journal").is_err());
    assert!(
        exec(
            &db,
            "INSERT INTO lan_journal (terminal_id, seq, id, at, action, entity_type, payload, prev_hash, hash,
                                      intact, received_at, merge_prev, merge_hash)
             SELECT terminal_id, seq, '00000000-0000-7000-8000-00000000e002', at, action, entity_type, payload,
                    prev_hash, hash, intact, received_at, merge_prev, merge_hash FROM lan_journal"
        )
        .is_err(),
        "one row per (till, seq)"
    );
}

#[test]
fn one_open_notice_per_subject_and_only_the_acknowledgement_changes() {
    let db = Db::open_memory().unwrap();
    let notice = |id: &'static str| {
        let sql: &'static str = Box::leak(
            format!(
                "INSERT INTO lan_notices (id, kind, terminal_id, subject, details, created_at)
                 VALUES ('{id}', 'oversold', '00000000-0000-7000-8000-0000000000c2',
                         '00000000-0000-7000-8000-00000000d001', '{{}}', 1)"
            )
            .into_boxed_str(),
        );
        exec(&db, sql)
    };
    notice("00000000-0000-7000-8000-00000000c101").unwrap();
    assert!(
        notice("00000000-0000-7000-8000-00000000c102").is_err(),
        "one open notice"
    );
    assert!(exec(&db, "UPDATE lan_notices SET details = '{\"x\":1}'").is_err());
    assert!(
        exec(&db, "UPDATE lan_notices SET acknowledged_at = 2").is_err(),
        "who acknowledged"
    );
    exec(
        &db,
        "UPDATE lan_notices SET acknowledged_at = 2, acknowledged_by = 'u1'",
    )
    .unwrap();
    notice("00000000-0000-7000-8000-00000000c102").unwrap();
    assert!(exec(&db, "DELETE FROM lan_notices").is_err());
    assert!(
        exec(
            &db,
            "INSERT INTO lan_notices (id, kind, subject, details, created_at)
             VALUES ('00000000-0000-7000-8000-00000000c103', 'gossip', 's', '{}', 1)"
        )
        .is_err(),
        "known kinds only"
    );
}

#[test]
fn the_change_feed_is_append_only_and_needs_a_positioned_consumer() {
    let db = Db::open_memory().unwrap();
    exec(
        &db,
        "INSERT INTO changelog (at, table_name, row_id, op, payload) VALUES (1, 'products', 'p1', 'update', '{}')",
    )
    .unwrap();
    assert!(exec(&db, "UPDATE changelog SET op = 'delete'").is_err());
    exec(&db, "DELETE FROM changelog").unwrap(); // pruning is how readers let go of rows
    assert!(
        exec(
            &db,
            "INSERT INTO sync_cursors (name, kind, updated_at) VALUES ('changelog:lan:outbox', 'changelog', 0)"
        )
        .is_err(),
        "a changelog consumer has a position"
    );
}

fn newest_fixture() -> (PathBuf, i64) {
    // The previous release's database: M3 writes m2-back-office.db (schema 3); a later fixture
    // written by M4 wins. Update the pair if the fixtures folder holds a newer one.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for (name, schema) in [
        ("m4-hardware.db", 5),
        ("m3-documents.db", 4),
        ("m2-back-office.db", 3),
    ] {
        let path = dir.join(name);
        if path.exists() {
            return (path, schema);
        }
    }
    panic!("no fixture of a previous release in tests/fixtures");
}

#[test]
fn the_previous_release_upgrades_standalone_with_an_empty_feed() {
    let (fixture, schema) = newest_fixture();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    std::fs::copy(fixture, &path).unwrap();
    let db = Db::open(
        &path,
        DbOptions {
            readers: 1,
            backup_dir: Some(dir.path().join("backups")),
        },
    )
    .unwrap();
    assert_eq!(db.report().schema_version, 9);
    assert_eq!(db.report().migrations_applied, 9 - schema);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM changelog"), 0);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM lan_outbox"), 0);
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM lan_local WHERE role = 'standalone'"
        ),
        1
    );
    let chains = db.read(documents::verify_all).unwrap();
    assert!(
        chains.iter().all(|c| c.first_broken_seq.is_none()),
        "{chains:?}"
    );
    assert!(
        db.read(audit::verify_chain)
            .unwrap()
            .first_broken_seq
            .is_none()
    );
}

/// A later migration on a paired till: it writes a shared table (a till's guard refuses that
/// with `host_required`) and drops a column the till's capture triggers name. The LAN triggers
/// go before the migrations run, and the start sequence's `capture::ensure` puts them back.
#[test]
fn a_till_runs_a_later_migration_and_gets_its_triggers_back() {
    use crate::sync::capture::{self, CaptureSet};
    use crate::sync::testkit::{LanPair, lan_pair};
    let LanPair {
        dir,
        host,
        till,
        till_terminal,
        ..
    } = lan_pair();
    drop(host);
    let path = dir.path().join("C2.db");
    assert!(
        exec(
            &till,
            "INSERT INTO units (id, code, name_fr, created_at, updated_at)
             VALUES ('00000000-0000-7000-8000-00000000b0b0', 'sonde', 'Sonde', 0, 0)"
        )
        .unwrap_err()
        .code()
            == "host_required",
        "the till's guard is on"
    );
    till.close().unwrap();
    let mut list = super::migrations::LIST.to_vec();
    list.push(rusqlite_migration::M::up(
        "INSERT INTO units (id, code, name_fr, created_at, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000b0b0', 'sonde', 'Sonde', 0, 0);
         ALTER TABLE expenses DROP COLUMN note;",
    ));
    let migrations = rusqlite_migration::Migrations::new(list);
    let latest = super::migrations::LATEST_VERSION + 1;
    let db = Db::open_with(
        &path,
        DbOptions {
            readers: 1,
            backup_dir: Some(dir.path().join("till-backups")),
        },
        &migrations,
        latest,
    )
    .unwrap();
    assert_eq!(db.report().schema_version, latest);
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM units WHERE code = 'sonde'"),
        1
    );
    assert!(
        db.read(capture::installed).unwrap().is_empty(),
        "dropped for the migration"
    );
    let id = till_terminal.id.clone();
    let reinstalled = db
        .write(move |tx| capture::ensure(tx, &CaptureSet::Client { terminal_id: id }))
        .unwrap();
    assert!(reinstalled);
    let names = db.read(capture::installed).unwrap();
    assert!(
        names.iter().any(|n| n == "lan_guard_units_insert"),
        "{names:?}"
    );
    assert!(
        names.iter().any(|n| n == "lan_cap_expenses_insert"),
        "{names:?}"
    );
    assert_eq!(
        exec(
            &db,
            "INSERT INTO units (id, code, name_fr, created_at, updated_at)
             VALUES ('00000000-0000-7000-8000-00000000b0b1', 'sonde2', 'Sonde', 0, 0)"
        )
        .unwrap_err()
        .code(),
        "host_required",
        "the guards are back"
    );
}
