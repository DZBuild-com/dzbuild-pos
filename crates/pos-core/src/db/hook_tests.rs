//! The LAN hooks of `Db`: the write hook runs inside each write and can refuse it, replica
//! writes run without triggers and put them back, a snapshot is a consistent single file.
//! Refusals use `forced`, the fixture code `every_emitted_code_is_known` allows in tests; the
//! hook's call counts tell which side refused.

use super::*;
use rusqlite::config::DbConfig;
use std::sync::atomic::AtomicUsize;

struct Recorder {
    before: AtomicUsize,
    after: AtomicUsize,
    refuse: AtomicBool,
}

impl WriteHook for Recorder {
    fn before_commit(&self, tx: &Transaction<'_>) -> Result<(), AppError> {
        self.before.fetch_add(1, Ordering::SeqCst);
        meta_set(tx, "hook_saw", "1")?;
        if self.refuse.load(Ordering::SeqCst) {
            return Err(AppError::conflict("forced"));
        }
        Ok(())
    }

    fn after_commit(&self) {
        self.after.fetch_add(1, Ordering::SeqCst);
    }
}

fn recorder() -> Arc<Recorder> {
    Arc::new(Recorder {
        before: AtomicUsize::new(0),
        after: AtomicUsize::new(0),
        refuse: AtomicBool::new(false),
    })
}

fn meta(db: &Db, key: &'static str) -> Option<String> {
    db.read(move |c| meta_get(c, key)).unwrap()
}

#[test]
fn the_hook_runs_inside_each_write_and_its_error_rolls_the_write_back() {
    let db = Db::open_memory().unwrap();
    assert!(!db.has_write_hook());
    let hook = recorder();
    db.set_write_hook(Some(hook.clone()));
    assert!(db.has_write_hook());
    db.write(|tx| meta_set(tx, "written", "a")).unwrap();
    assert_eq!(
        meta(&db, "hook_saw").as_deref(),
        Some("1"),
        "same transaction"
    );
    assert_eq!(
        (
            hook.before.load(Ordering::SeqCst),
            hook.after.load(Ordering::SeqCst)
        ),
        (1, 1)
    );
    hook.refuse.store(true, Ordering::SeqCst);
    let err = db.write(|tx| meta_set(tx, "written", "b")).unwrap_err();
    assert_eq!(err.code(), "forced");
    assert_eq!(hook.before.load(Ordering::SeqCst), 2, "the hook refused");
    assert_eq!(
        meta(&db, "written").as_deref(),
        Some("a"),
        "the write rolled back"
    );
    assert_eq!(
        hook.after.load(Ordering::SeqCst),
        1,
        "no commit, no after_commit"
    );
    let failed = db.write(|_| Err::<(), _>(AppError::conflict("forced")));
    assert_eq!(failed.unwrap_err().code(), "forced");
    assert_eq!(
        hook.before.load(Ordering::SeqCst),
        2,
        "a failing closure never reaches the hook"
    );
    db.set_write_hook(None);
    db.write(|tx| meta_set(tx, "written", "c")).unwrap();
    assert_eq!(hook.before.load(Ordering::SeqCst), 2);
}

#[test]
fn dry_runs_and_replica_writes_never_call_the_hook() {
    let db = Db::open_memory().unwrap();
    let hook = recorder();
    db.set_write_hook(Some(hook.clone()));
    db.dry_run(|tx| meta_set(tx, "k", "v")).unwrap();
    db.write_replica(|tx| meta_set(tx, "k", "v")).unwrap();
    assert_eq!(hook.before.load(Ordering::SeqCst), 0);
    assert_eq!(
        meta(&db, "k").as_deref(),
        Some("v"),
        "the replica write committed"
    );
}

const PRODUCT: &str = "
    INSERT INTO tax_rates (id, code, rate_bp, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000e401', 'TVA19', 1900, 0, 0);
    INSERT INTO products (id, name_fr, unit_id, tax_rate_id, sell_price, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000e001', 'Lait', '00000000-0000-7000-8000-000000000101',
              '00000000-0000-7000-8000-00000000e401', 10000, 0, 0);";

const MOVE: &str = "INSERT INTO stock_movements (id, product_id, location_id, qty_delta, reason, created_at)
    VALUES (?1, '00000000-0000-7000-8000-00000000e001', '00000000-0000-7000-8000-000000000301', 1000, 'opening', 1)";

fn level(db: &Db) -> i64 {
    db.read(|c| {
        Ok(
            c.query_row("SELECT COALESCE(SUM(qty), 0) FROM stock_levels", [], |r| {
                r.get(0)
            })?,
        )
    })
    .unwrap()
}

#[test]
fn replica_writes_run_without_triggers_and_cached_statements_get_them_back() {
    let db = Db::open_memory().unwrap();
    db.write(|tx| Ok(tx.execute_batch(PRODUCT)?)).unwrap();
    let insert = |db: &Db, replica: bool| {
        let id = crate::ids::new_id();
        let run = move |tx: &mut Transaction<'_>| {
            tx.prepare_cached(MOVE)?.execute([&id])?;
            Ok(())
        };
        if replica {
            db.write_replica(run).unwrap();
        } else {
            db.write(run).unwrap();
        }
    };
    insert(&db, false);
    assert_eq!(level(&db), 1_000, "the level trigger fired");
    insert(&db, true);
    assert_eq!(level(&db), 1_000, "no trigger in a replica write");
    insert(&db, false);
    assert_eq!(
        level(&db),
        2_000,
        "the cached statement recompiled with its trigger"
    );
    db.write_replica(|tx| {
        tx.execute("UPDATE counters SET last_number = 5", [])?; // no rows: just must not fail
        tx.execute(
            "INSERT INTO counters (doc_type, series, fiscal_year, last_number) VALUES ('ticket', 'C2', 2026, 7)",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let jump = db.write(|tx| {
        Ok(tx.execute(
            "UPDATE counters SET last_number = 9 WHERE series = 'C2'",
            [],
        )?)
    });
    assert_eq!(
        jump.unwrap_err().code(),
        "constraint_violation",
        "triggers are back on"
    );
    let failed = db.write_replica(|_| Err::<(), _>(AppError::conflict("forced")));
    assert_eq!(failed.unwrap_err().code(), "forced");
    insert(&db, false);
    assert_eq!(
        level(&db),
        3_000,
        "triggers are back after a failed replica write too"
    );
}

#[test]
fn a_write_refuses_to_run_with_triggers_off() {
    let db = Db::open_memory().unwrap();
    db.write(|tx| {
        tx.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)?;
        Ok(())
    })
    .unwrap();
    let err = db.write(|_| Ok(())).unwrap_err();
    assert_eq!(err.code(), "internal_error");
}

#[test]
fn a_snapshot_is_a_consistent_single_file_copy() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("pos.db"), DbOptions::default()).unwrap();
    db.write(|tx| Ok(tx.execute_batch(PRODUCT)?)).unwrap();
    let copy = dir.path().join("snapshot.db");
    db.snapshot_to(&copy).unwrap();
    assert!(
        !dir.path().join("snapshot.db-wal").exists(),
        "a single file"
    );
    let conn = Connection::open(&copy).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM products", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
    assert_eq!(
        Db::open_memory()
            .unwrap()
            .snapshot_to(&copy)
            .unwrap_err()
            .code(),
        "memory_database"
    );
}
