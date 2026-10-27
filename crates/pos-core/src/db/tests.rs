use super::*;
use tempfile::TempDir;

fn file_db() -> (TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("pos.db"), DbOptions::default()).unwrap();
    (dir, db)
}

fn count(db: &Db, sql: &'static str) -> i64 {
    db.read(move |c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .unwrap()
}

#[test]
fn migrations_are_valid() {
    migrations::MIGRATIONS.validate().unwrap();
    assert_eq!(migrations::LATEST_VERSION, 10);
}

#[test]
fn open_memory_creates_exactly_the_tables_up_to_m8() {
    let db = Db::open_memory().unwrap();
    let tables: Vec<String> = db
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )?;
            let names = stmt.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
            Ok(names)
        })
        .unwrap();
    assert_eq!(
        tables,
        [
            "app_meta",
            "appointments",
            "audit_log",
            "backups_log",
            "cash_closures",
            "cash_counts",
            "cash_movements",
            "cash_sessions",
            "categories",
            "category_stations",
            "changelog",
            "chifa_prices",
            "cloud_backup_keys",
            "cloud_backups",
            "cloud_endpoints",
            "cloud_inbox",
            "cloud_links",
            "cloud_map",
            "cloud_notices",
            "cloud_publish",
            "cloud_stock",
            "cloud_variant_map",
            "company",
            "controlled_register",
            "cost_history",
            "counter_skips",
            "counters",
            "customer_allocations",
            "customer_consents",
            "customer_ledger",
            "customer_orders",
            "customer_prices",
            "customers",
            "daily_menu",
            "delivery_round_stops",
            "delivery_rounds",
            "department_keys",
            "deposits",
            "dining_lines",
            "dining_rooms",
            "dining_tables",
            "dining_tickets",
            "document_drafts",
            "document_lines",
            "document_links",
            "document_parties",
            "document_taxes",
            "documents",
            "expense_categories",
            "expenses",
            "flexy_float",
            "gift_voucher_ledger",
            "gift_vouchers",
            "held_carts",
            "import_job_rows",
            "import_jobs",
            "instalment_dues",
            "instalment_events",
            "instalment_guarantees",
            "instalment_plans",
            "jeweller_register",
            "jewellery_pieces",
            "kitchen_stations",
            "kitchen_tickets",
            "label_prints",
            "lan_commits",
            "lan_devices",
            "lan_inbox",
            "lan_journal",
            "lan_local",
            "lan_notices",
            "lan_outbox",
            "location_minimums",
            "lot_levels",
            "lots",
            "loyalty_cards",
            "loyalty_ledger",
            "metal_prices",
            "modifier_groups",
            "modifier_options",
            "online_order_lines",
            "online_orders",
            "open_price_products",
            "order_events",
            "payment_type_uses",
            "payment_types",
            "payments",
            "pharmacy_products",
            "price_breaks",
            "price_levels",
            "product_barcodes",
            "product_components",
            "product_extras",
            "product_modifier_groups",
            "product_prices",
            "product_regulations",
            "product_wholesale",
            "products",
            "products_fts",
            "products_fts_config",
            "products_fts_data",
            "products_fts_docsize",
            "products_fts_idx",
            "promotion_products",
            "promotions",
            "remote_customer_flags",
            "roles",
            "sale_line_promotions",
            "scale_items",
            "serials",
            "service_ticket_events",
            "service_ticket_items",
            "service_tickets",
            "settings",
            "staff_ledger",
            "staff_profiles",
            "stock_count_lines",
            "stock_count_lot_lines",
            "stock_counts",
            "stock_levels",
            "stock_locations",
            "stock_movements",
            "stock_reservations",
            "stock_transfers",
            "supplier_allocations",
            "supplier_ledger",
            "suppliers",
            "sync_cursors",
            "sync_outbox",
            "tax_rates",
            "terminals",
            "units",
            "users",
            "yield_run_cuts",
            "yield_runs",
            "yield_template_cuts",
            "yield_templates",
            "zakat_statements"
        ]
    );
    assert!(db.read(|c| meta_get(c, META_INSTALL_ID)).unwrap().is_some());
}

#[test]
fn file_db_applies_the_pragmas() {
    let (_dir, db) = file_db();
    let (mode, fk, busy): (String, i64, i64) = db
        .read(|c| {
            Ok((
                c.pragma_query_value(None, "journal_mode", |r| r.get(0))?,
                c.pragma_query_value(None, "foreign_keys", |r| r.get(0))?,
                c.pragma_query_value(None, "busy_timeout", |r| r.get(0))?,
            ))
        })
        .unwrap();
    assert_eq!((mode.as_str(), fk, busy), ("wal", 1, 5_000));
    let sync: i64 = db
        .write(|tx| Ok(tx.pragma_query_value(None, "synchronous", |r| r.get(0))?))
        .unwrap();
    assert_eq!(sync, 2, "synchronous=FULL on the writer");
    assert_eq!(db.report().schema_version, 10);
}

#[test]
fn writer_serialises_concurrent_writes() {
    let (_dir, db) = file_db();
    let handles: Vec<_> = (0..8)
        .map(|t| {
            let db = db.clone();
            std::thread::spawn(move || {
                for i in 0..50 {
                    db.write(move |tx| {
                        tx.execute(
                            "INSERT INTO app_meta (key, value) VALUES (?1, 'x')",
                            [format!("k-{t}-{i}")],
                        )?;
                        Ok(())
                    })
                    .unwrap();
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM app_meta WHERE key LIKE 'k-%'"),
        400
    );
}

#[test]
fn failing_write_rolls_back() {
    let db = Db::open_memory().unwrap();
    let err = db
        .write(|tx| {
            tx.execute(
                "INSERT INTO app_meta (key, value) VALUES ('rollback', 'x')",
                [],
            )?;
            Err::<(), _>(AppError::validation("probe", "forced"))
        })
        .unwrap_err();
    assert_eq!(err.code(), "forced");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM app_meta WHERE key = 'rollback'"),
        0
    );
}

#[test]
fn writer_keeps_working_after_a_failed_write() {
    let db = Db::open_memory().unwrap();
    let err = db
        .write(|_tx| Err::<(), _>(AppError::validation("probe", "forced")))
        .unwrap_err();
    assert_eq!(err.code(), "forced");
    db.write(|tx| meta_set(tx, "after_error", "ok")).unwrap();
    assert_eq!(
        db.read(|c| meta_get(c, "after_error")).unwrap().as_deref(),
        Some("ok")
    );
}

#[test]
fn reads_see_committed_writes_on_file_db() {
    let (_dir, db) = file_db();
    db.write(|tx| meta_set(tx, "probe", "42")).unwrap();
    assert_eq!(
        db.read(|c| meta_get(c, "probe")).unwrap().as_deref(),
        Some("42")
    );
}

#[test]
fn newer_database_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();
    let err = Db::open(&path, DbOptions::default()).unwrap_err();
    assert_eq!(err.code(), "database_newer_than_app");
}

#[test]
fn corrupt_file_is_reported_not_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    std::fs::write(&path, vec![0x42_u8; 8_192]).unwrap();
    let err = Db::open(&path, DbOptions::default()).unwrap_err();
    assert_eq!(err.code(), "database_corrupt");
}

#[test]
fn every_write_runs_on_the_one_writer_thread() {
    let (_dir, db) = file_db();
    let callers: Vec<_> = (0..4)
        .map(|_| {
            let db = db.clone();
            std::thread::spawn(move || {
                let ran_on: Vec<_> = (0..10)
                    .map(|_| db.write(|_tx| Ok(std::thread::current().id())).unwrap())
                    .collect();
                (std::thread::current().id(), ran_on)
            })
        })
        .collect();
    let mut writers = std::collections::HashSet::new();
    for handle in callers {
        let (caller, ran_on) = handle.join().unwrap();
        assert!(
            !ran_on.contains(&caller),
            "a write ran on its caller's thread"
        );
        writers.extend(ran_on);
    }
    assert_eq!(
        writers.len(),
        1,
        "writes from 4 callers ran on {} threads",
        writers.len()
    );
}

#[test]
fn dropping_the_last_handle_checkpoints_the_wal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    let db = Db::open(&path, DbOptions::default()).unwrap();
    let clone = db.clone();
    db.write(|tx| meta_set(tx, "probe", "kept")).unwrap();
    assert_eq!(
        db.read(|c| meta_get(c, "probe")).unwrap().as_deref(),
        Some("kept")
    );
    drop(db);
    assert!(
        dir.path().join("pos.db-wal").exists(),
        "a live clone keeps the database open"
    );
    drop(clone);
    assert!(!dir.path().join("pos.db-wal").exists(), "WAL left behind");
    assert!(
        !dir.path().join("pos.db-shm").exists(),
        "shared memory file left behind"
    );
    let plain = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(meta_get(&plain, "probe").unwrap().as_deref(), Some("kept"));
}

#[test]
fn open_describes_paths_and_report() {
    fn shareable<T: Clone + std::fmt::Debug + Send + Sync + 'static>(_: &T) {}
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shop").join("pos.db");
    let db = Db::open(&path, DbOptions::default()).unwrap();
    shareable(&db);
    assert!(format!("{db:?}").contains("pos.db"));
    assert_eq!(db.path(), Some(path.as_path()));
    assert_eq!(
        db.backup_dir(),
        Some(dir.path().join("shop").join("backups").as_path())
    );
    let fresh = OpenReport {
        unclean_shutdown: false,
        integrity_ok: true,
        schema_version: 10,
        migrations_applied: 10,
        pre_migration_backup: None,
    };
    assert_eq!(db.report(), &fresh);
    let install_id = db.read(|c| meta_get(c, META_INSTALL_ID)).unwrap();
    db.close().unwrap();

    // Reopen after a clean close: nothing left to migrate, same install id, clean start.
    let custom = dir.path().join("elsewhere");
    let opts = DbOptions {
        readers: 0,
        backup_dir: Some(custom.clone()),
    };
    let db = Db::open(&path, opts.clone()).unwrap();
    assert_eq!(db.backup_dir(), Some(custom.as_path()));
    let reopened = OpenReport {
        migrations_applied: 0,
        ..fresh
    };
    assert_eq!(db.report(), &reopened);
    assert_eq!(
        db.read(|c| meta_get(c, META_INSTALL_ID)).unwrap(),
        install_id
    );
    drop(db);

    // Reopen after a drop without close (a power cut): unclean, so the integrity check ran.
    let db = Db::open(&path, opts).unwrap();
    let after_power_cut = OpenReport {
        unclean_shutdown: true,
        ..reopened
    };
    assert_eq!(db.report(), &after_power_cut);
    drop(db);

    let memory = Db::open_memory().unwrap();
    assert_eq!((memory.path(), memory.backup_dir()), (None, None));
    assert_eq!(memory.report().migrations_applied, 10);
}

#[test]
fn every_table_is_strict() {
    let db = Db::open_memory().unwrap();
    let loose = count(
        &db,
        "SELECT COUNT(*) FROM pragma_table_list
         WHERE schema = 'main' AND type = 'table' AND strict = 0 AND name NOT LIKE 'sqlite_%'",
    );
    assert_eq!(loose, 0);
}

#[test]
fn audit_log_rejects_update_and_delete() {
    let db = Db::open_memory().unwrap();
    db.write(|tx| {
        tx.execute(
            "INSERT INTO audit_log (id, at, action, entity_type, payload, prev_hash, hash)
             VALUES (?1, 0, 'probe', 'probe', '{}', ?2, ?2)",
            (crate::ids::new_id(), "0".repeat(64)),
        )?;
        Ok(())
    })
    .unwrap();
    let update = db.write(|tx| Ok(tx.execute("UPDATE audit_log SET action = 'edited'", [])?));
    let delete = db.write(|tx| Ok(tx.execute("DELETE FROM audit_log", [])?));
    assert_eq!(update.unwrap_err().code(), "constraint_violation");
    assert_eq!(delete.unwrap_err().code(), "constraint_violation");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM audit_log WHERE action = 'probe'"),
        1
    );
}

#[test]
fn company_nif_length_is_left_to_rust() {
    let db = Db::open_memory().unwrap();
    for nif in ["123456789012345", "123456789012345678"] {
        db.write(move |tx| {
            tx.execute(
                "INSERT INTO company (id, nif, updated_at) VALUES (1, ?1, 0)
                 ON CONFLICT (id) DO UPDATE SET nif = excluded.nif",
                [nif],
            )?;
            Ok(())
        })
        .unwrap();
    }
    assert_eq!(
        count(&db, "SELECT length(nif) FROM company WHERE id = 1"),
        18
    );
}

/// A database file closed without the clean-shutdown mark, holding 2 000 `settings` rows.
fn filled_db_file(dir: &Path) -> PathBuf {
    let path = dir.join("pos.db");
    let db = Db::open(&path, DbOptions::default()).unwrap();
    db.write(|tx| {
        for i in 0..2_000 {
            tx.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1, '\"filler\"', 0)",
                [format!("filler.{i:05}")],
            )?;
        }
        Ok(())
    })
    .unwrap();
    path
}

#[test]
fn damage_found_by_the_integrity_check_is_reported() {
    // A key changed in the table but not in its index (same length, same sort order):
    // integrity_check answers with a list of problems instead of "ok".
    let dir = tempfile::tempdir().unwrap();
    let path = filled_db_file(dir.path());
    let mut bytes = std::fs::read(&path).unwrap();
    let at = bytes
        .windows(12)
        .position(|w| w == b"filler.01000")
        .unwrap();
    bytes[at + 11] = b' ';
    std::fs::write(&path, &bytes).unwrap();
    let err = Db::open(&path, DbOptions::default()).unwrap_err();
    assert_eq!(err.code(), "database_corrupt");

    // A torn page: integrity_check itself fails with SQLITE_CORRUPT.
    let dir = tempfile::tempdir().unwrap();
    let path = filled_db_file(dir.path());
    let mut bytes = std::fs::read(&path).unwrap();
    let last_page = bytes.len() - 4_096;
    bytes[last_page..].fill(0xFF);
    std::fs::write(&path, &bytes).unwrap();
    let err = Db::open(&path, DbOptions::default()).unwrap_err();
    assert_eq!(err.code(), "database_corrupt");
}

// ---- Review round 1 ----

#[test]
fn audit_log_rejects_insert_or_replace() {
    let db = Db::open_memory().unwrap();
    let (id, hash) = (crate::ids::new_id(), "0".repeat(64));
    let (row_id, row_hash) = (id.clone(), hash.clone());
    db.write(move |tx| {
        tx.execute(
            "INSERT INTO audit_log (seq, id, at, action, entity_type, payload, prev_hash, hash)
             VALUES (1, ?1, 0, 'probe', 'probe', '{}', ?2, ?2)",
            (row_id, row_hash),
        )?;
        Ok(())
    })
    .unwrap();
    // REPLACE deletes the conflicting row first: on the same `seq`, then on the same `id`.
    for (seq, replacement_id) in [(1, crate::ids::new_id()), (2, id.clone())] {
        let hash = hash.clone();
        let err = db
            .write(move |tx| {
                Ok(tx.execute(
                    "INSERT OR REPLACE INTO audit_log
                     (seq, id, at, action, entity_type, payload, prev_hash, hash)
                     VALUES (?1, ?2, 0, 'replaced', 'probe', '{}', ?3, ?3)",
                    (seq, replacement_id, hash),
                )?)
            })
            .unwrap_err();
        assert_eq!(err.code(), "constraint_violation", "seq {seq}");
    }
    assert_eq!(count(&db, "SELECT COUNT(*) FROM audit_log"), 1);
    let row: (i64, String, String) = db
        .read(|c| {
            Ok(
                c.query_row("SELECT seq, id, action FROM audit_log", [], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })?,
            )
        })
        .unwrap();
    assert_eq!(row, (1, id, "probe".to_owned()));
}

#[test]
fn re_entrant_calls_from_the_writer_fail_instead_of_deadlocking() {
    let db = Db::open_memory().unwrap();
    let nested = db.clone();
    let (write, read) = db
        .write(move |_tx| Ok((nested.write(|_tx| Ok(())), nested.read(|_c| Ok(())))))
        .unwrap();
    let re_entrant = AppError::Internal("re-entrant database call".into());
    assert_eq!(write.unwrap_err(), re_entrant);
    assert_eq!(read.unwrap_err(), re_entrant);
    db.write(|tx| meta_set(tx, "after_re_entry", "ok")).unwrap();
    assert_eq!(
        db.read(|c| meta_get(c, "after_re_entry"))
            .unwrap()
            .as_deref(),
        Some("ok")
    );
}

#[test]
fn reads_cannot_write_on_memory_or_file_db() {
    let (_dir, file) = file_db();
    for db in [Db::open_memory().unwrap(), file] {
        let sneaky = db.read(|c| {
            Ok(c.execute(
                "INSERT INTO app_meta (key, value) VALUES ('sneaky', 'x')",
                [],
            )?)
        });
        assert_eq!(sneaky.unwrap_err().code(), "database_error", "{db:?}");
        // The failed read left the in-memory writer writable again.
        db.write(|tx| meta_set(tx, "after_read", "ok")).unwrap();
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM app_meta WHERE key = 'sneaky'"),
            0
        );
        assert_eq!(
            db.read(|c| meta_get(c, "after_read")).unwrap().as_deref(),
            Some("ok")
        );
    }
}

fn role_pragmas(c: &Connection) -> Result<(i64, i64), AppError> {
    Ok((
        c.pragma_query_value(None, "query_only", |r| r.get(0))?,
        c.pragma_query_value(None, "recursive_triggers", |r| r.get(0))?,
    ))
}

#[test]
fn readers_are_query_only_and_every_connection_fires_recursive_triggers() {
    let (_dir, file) = file_db();
    for db in [file, Db::open_memory().unwrap()] {
        assert_eq!(db.read(role_pragmas).unwrap(), (1, 1), "reader of {db:?}");
        assert_eq!(
            db.write(|tx| role_pragmas(tx)).unwrap(),
            (0, 1),
            "writer of {db:?}"
        );
    }
}

#[test]
fn configure_refuses_a_file_db_that_stays_out_of_wal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nolock.db");
    // `nolock=1` disables WAL support: the switch answers with the old mode, "delete".
    let path = path
        .display()
        .to_string()
        .replace(std::path::MAIN_SEPARATOR, "/");
    let conn = rusqlite::Connection::open(format!("file:{path}?nolock=1")).unwrap();
    let err = configure(&conn, Role::Writer).unwrap_err();
    assert_eq!(
        err.payload().message.as_deref(),
        Some("journal_mode is delete, expected wal")
    );
    drop(conn);
    // An in-memory database legitimately answers "memory".
    configure(
        &rusqlite::Connection::open_in_memory().unwrap(),
        Role::Writer,
    )
    .unwrap();
}

#[test]
fn transactions_left_open_are_rolled_back() {
    // In-memory: the read runs on the writer connection, and the next job must still write.
    let db = Db::open_memory().unwrap();
    db.read(|c| Ok(c.execute_batch("BEGIN")?)).unwrap();
    db.write(|tx| meta_set(tx, "after_begin", "ok")).unwrap();

    // File: with one reader, a snapshot left open by a read must not survive its lease.
    let dir = tempfile::tempdir().unwrap();
    let opts = DbOptions {
        readers: 1,
        backup_dir: None,
    };
    let db = Db::open(&dir.path().join("pos.db"), opts).unwrap();
    db.read(|c| {
        c.execute_batch("BEGIN")?;
        Ok(c.query_row("SELECT COUNT(*) FROM app_meta", [], |r| r.get::<_, i64>(0))?)
    })
    .unwrap();
    db.write(|tx| meta_set(tx, "fresh", "1")).unwrap();
    assert_eq!(
        db.read(|c| meta_get(c, "fresh")).unwrap().as_deref(),
        Some("1")
    );
}

#[test]
fn writes_enforce_foreign_keys() {
    let (_dir, db) = file_db();
    let role_id = crate::ids::new_id();
    let known = role_id.clone();
    db.write(move |tx| {
        tx.execute(
            "INSERT INTO roles (id, name_ar, name_fr, name_en, created_at, updated_at)
             VALUES (?1, 'r', 'r', 'r', 0, 0)",
            [known],
        )?;
        Ok(())
    })
    .unwrap();
    let add_user = |username: &'static str, role_id: String| {
        db.write(move |tx| {
            tx.execute(
                "INSERT INTO users (id, username, display_name, pin_hash, role_id, created_at, updated_at)
                 VALUES (?1, ?2, ?2, 'hash', ?3, 0, 0)",
                (crate::ids::new_id(), username, role_id),
            )?;
            Ok(())
        })
    };
    add_user("known", role_id).unwrap();
    let orphan = add_user("orphan", crate::ids::new_id()).unwrap_err();
    assert_eq!(orphan.code(), "constraint_violation");
    assert_eq!(count(&db, "SELECT COUNT(*) FROM users"), 1);
}

#[test]
fn company_is_a_single_row() {
    let db = Db::open_memory().unwrap();
    let insert = |id: i64| {
        db.write(move |tx| {
            tx.execute("INSERT INTO company (id, updated_at) VALUES (?1, 0)", [id])?;
            Ok(())
        })
    };
    insert(1).unwrap();
    assert_eq!(insert(2).unwrap_err().code(), "constraint_violation");
    assert_eq!(count(&db, "SELECT COUNT(*) FROM company"), 1);
}

#[test]
fn newer_database_is_refused_before_it_is_touched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();
    let err = Db::open(&path, DbOptions::default()).unwrap_err();
    assert_eq!(err.code(), "database_newer_than_app");
    let conn = rusqlite::Connection::open(&path).unwrap();
    let mode: String = conn
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "delete", "the newer app's journal mode was changed");
}

#[test]
fn dry_run_returns_the_result_and_keeps_nothing() {
    let db = Db::open_memory().unwrap();
    let seen = db
        .dry_run(|tx| {
            tx.execute(
                "INSERT INTO app_meta (key, value) VALUES ('probe', 'x')",
                [],
            )?;
            Ok(
                tx.query_row("SELECT value FROM app_meta WHERE key = 'probe'", [], |r| {
                    r.get::<_, String>(0)
                })?,
            )
        })
        .unwrap();
    assert_eq!(seen, "x", "the closure saw its own write");
    let kept: i64 = db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM app_meta WHERE key = 'probe'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(kept, 0, "rolled back");
    let failed = db.dry_run(|_| Err::<(), _>(AppError::conflict("cart_empty")));
    assert_eq!(failed.unwrap_err().code(), "cart_empty");
    db.write(|tx| {
        Ok(tx.execute(
            "INSERT INTO app_meta (key, value) VALUES ('after', 'y')",
            [],
        )?)
    })
    .unwrap();
}

/// A dry run never leaves an audit row behind, and the journal it saw is
/// the one the next real write extends (no `seq` burnt, the chain stays whole).
#[test]
fn dry_run_keeps_no_audit_row_and_the_chain_stays_whole() {
    use crate::audit::{self, Actor};
    let db = Db::open_memory().unwrap();
    let probe = || {
        Actor::default().event(
            "settings.updated",
            "settings",
            None,
            serde_json::json!({ "probe": true }),
        )
    };
    let seen = db
        .dry_run(move |tx| Ok(audit::append(tx, &probe())?.seq))
        .unwrap();
    assert_eq!(seen, 1, "the closure saw its own audit row");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM audit_log"),
        0,
        "rolled back"
    );
    let kept = db
        .write(move |tx| Ok(audit::append(tx, &probe())?.seq))
        .unwrap();
    assert_eq!(kept, 1, "the rolled-back row burnt no seq");
    let report = db.read(audit::verify_chain).unwrap();
    assert_eq!((report.rows, report.first_broken_seq), (1, None));
}
