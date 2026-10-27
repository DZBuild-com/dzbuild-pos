//! Migration 0002 (catalogue, stock, documents, payments, cash): seeds, append-only ledgers,
//! immutability of validated documents, FTS triggers, and the upgrade of an M0 database.

use super::*;
use crate::audit::{self, Actor};
use crate::company::{self, CompanyInput};
use crate::lang::Lang;
use rusqlite::Connection;
use serde_json::json;
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

/// A tax rate, one product, an open cash session and a draft ticket with one line and one
/// payment, written with plain SQL (the services come in later tasks).
fn seeded() -> Db {
    let db = Db::open_memory().unwrap();
    exec(
        &db,
        "INSERT INTO tax_rates (id, code, rate_bp, created_at, updated_at)
           VALUES ('00000000-0000-7000-8000-000000000401', 'TVA19', 1900, 0, 0);
         INSERT INTO products (id, name_fr, unit_id, tax_rate_id, sell_price, created_at, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000a001', 'Lait', '00000000-0000-7000-8000-000000000101',
                   '00000000-0000-7000-8000-000000000401', 11900, 0, 0);
         INSERT INTO cash_sessions (id, terminal_id, user_id, business_date, opened_at, opening_float)
           VALUES ('00000000-0000-7000-8000-00000000c001', 't1', 'u1', '2026-09-28', 0, 0);
         INSERT INTO documents (id, doc_type, series, business_date, user_id, terminal_id,
                                cash_session_id, subtotal_ht, tax_total, total_ttc, created_at, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000d001', 'ticket', 'C1', '2026-09-28', 'u1', 't1',
                   '00000000-0000-7000-8000-00000000c001', 10000, 1900, 11900, 0, 0);
         INSERT INTO document_lines (id, document_id, line_no, product_id, description, qty,
                                     unit_price, tax_rate_bp, total_ht, tax_amount, total_ttc)
           VALUES ('00000000-0000-7000-8000-00000000e001', '00000000-0000-7000-8000-00000000d001', 1,
                   '00000000-0000-7000-8000-00000000a001', 'Lait', 1000, 11900, 1900, 10000, 1900, 11900);
         INSERT INTO document_taxes (document_id, rate_bp, base_ht, tax_amount)
           VALUES ('00000000-0000-7000-8000-00000000d001', 1900, 10000, 1900);
         INSERT INTO payments (id, document_id, cash_session_id, method, amount, created_at, user_id, terminal_id)
           VALUES ('00000000-0000-7000-8000-00000000f001', '00000000-0000-7000-8000-00000000d001',
                   '00000000-0000-7000-8000-00000000c001', 'cash', 11900, 0, 'u1', 't1');",
    )
    .unwrap();
    db
}

const VALIDATE: &str = "UPDATE documents SET status = 'validated', number = 'TK2026-C1-000001',
     fiscal_year = 2026, validated_at = 1, chain_seq = 1,
     prev_hash = '0000000000000000000000000000000000000000000000000000000000000000',
     hash = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
     WHERE id = '00000000-0000-7000-8000-00000000d001'";

#[test]
fn seeds_units_price_levels_and_locations_with_fixed_ids() {
    let db = Db::open_memory().unwrap();
    let units: Vec<(String, i64)> = db
        .read(|c| {
            let mut stmt = c.prepare("SELECT code, decimals FROM units ORDER BY id")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap();
    let units: Vec<(&str, i64)> = units.iter().map(|(c, d)| (c.as_str(), *d)).collect();
    assert_eq!(
        units,
        [
            ("pc", 0),
            ("kg", 3),
            ("g", 0),
            ("l", 3),
            ("m", 3),
            ("m2", 3),
            ("box", 0),
            ("carton", 0),
            // 0008: the units the activity templates use.
            ("sachet", 0),
            ("m3", 3),
            ("bag", 0),
            ("pallet", 0),
            ("t", 3),
            ("lot", 0)
        ]
    );
    let levels: String = db
        .read(|c| {
            Ok(c.query_row(
                "SELECT group_concat(name_fr || ':' || is_default, ',') FROM
                   (SELECT name_fr, is_default FROM price_levels ORDER BY sort_order)",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(levels, "Détail:1,Demi-gros:0,Gros:0,Revendeur:0");
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM stock_locations WHERE is_default = 1 AND code = 'SHOP'"
        ),
        1
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM stock_locations WHERE kind = 'damaged'"
        ),
        1
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM tax_rates"),
        0,
        "seeded from the data pack"
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM counters"),
        0,
        "created lazily"
    );
}

#[test]
fn stock_ledger_is_append_only_and_moves_the_level_cache() {
    let db = seeded();
    exec(
        &db,
        "INSERT INTO stock_movements (id, product_id, location_id, qty_delta, reason, created_at)
           VALUES ('00000000-0000-7000-8000-00000000b001', '00000000-0000-7000-8000-00000000a001',
                   '00000000-0000-7000-8000-000000000301', 5000, 'opening', 10);
         INSERT INTO stock_movements (id, product_id, location_id, qty_delta, reason, created_at)
           VALUES ('00000000-0000-7000-8000-00000000b002', '00000000-0000-7000-8000-00000000a001',
                   '00000000-0000-7000-8000-000000000301', -1500, 'sale', 20);",
    )
    .unwrap();
    assert_eq!(count(&db, "SELECT qty FROM stock_levels"), 3_500);
    assert_eq!(count(&db, "SELECT updated_at FROM stock_levels"), 20);
    for sql in [
        "UPDATE stock_movements SET qty_delta = 1",
        "DELETE FROM stock_movements",
        "INSERT OR REPLACE INTO stock_movements (id, product_id, location_id, qty_delta, reason, created_at)
           VALUES ('00000000-0000-7000-8000-00000000b001', '00000000-0000-7000-8000-00000000a001',
                   '00000000-0000-7000-8000-000000000301', 1, 'opening', 30)",
    ] {
        assert_eq!(exec(&db, sql).unwrap_err().code(), "constraint_violation", "{sql}");
    }
    let zero =
        "INSERT INTO stock_movements (id, product_id, location_id, qty_delta, reason, created_at)
       VALUES ('00000000-0000-7000-8000-00000000b003', '00000000-0000-7000-8000-00000000a001',
               '00000000-0000-7000-8000-000000000301', 0, 'adjustment', 30)";
    assert_eq!(exec(&db, zero).unwrap_err().code(), "constraint_violation");
    assert_eq!(count(&db, "SELECT COUNT(*) FROM stock_movements"), 2);
}

#[test]
fn counters_start_at_one_and_only_step_up() {
    let db = Db::open_memory().unwrap();
    let next = "INSERT INTO counters (doc_type, series, fiscal_year, last_number) VALUES ('ticket', 'C1', 2026, 1)
       ON CONFLICT (doc_type, series, fiscal_year) DO UPDATE SET last_number = last_number + 1";
    for _ in 0..3 {
        exec(&db, next).unwrap();
    }
    assert_eq!(count(&db, "SELECT last_number FROM counters"), 3);
    for sql in [
        "UPDATE counters SET last_number = 2",
        "UPDATE counters SET last_number = 10",
        "DELETE FROM counters",
        "INSERT INTO counters (doc_type, series, fiscal_year, last_number) VALUES ('ticket', 'C2', 2026, 7)",
    ] {
        assert_eq!(
            exec(&db, sql).unwrap_err().code(),
            "constraint_violation",
            "{sql}"
        );
    }
}

#[test]
fn validated_documents_lines_taxes_and_payments_are_immutable() {
    let db = seeded();
    // A draft can still be corrected.
    exec(&db, "UPDATE document_lines SET description = 'Lait 1 L'").unwrap();
    exec(&db, VALIDATE).unwrap();
    for sql in [
        "UPDATE documents SET total_ttc = 1, subtotal_ht = 1, tax_total = 0",
        "UPDATE documents SET status = 'cancelled'",
        "DELETE FROM documents",
        "UPDATE document_lines SET qty = 2000",
        "DELETE FROM document_lines",
        "INSERT INTO document_lines (id, document_id, line_no, description, qty, unit_price,
            tax_rate_bp, total_ht, tax_amount, total_ttc)
          VALUES ('00000000-0000-7000-8000-00000000e002', '00000000-0000-7000-8000-00000000d001', 2,
                  'Extra', 1000, 100, 0, 100, 0, 100)",
        "UPDATE document_taxes SET tax_amount = 0",
        "DELETE FROM document_taxes",
        "UPDATE payments SET amount = 1",
        "DELETE FROM payments",
        "INSERT INTO payments (id, document_id, method, amount, created_at, user_id, terminal_id)
          VALUES ('00000000-0000-7000-8000-00000000f002', '00000000-0000-7000-8000-00000000d001',
                  'cash', 5, 0, 'u1', 't1')",
    ] {
        assert_eq!(
            exec(&db, sql).unwrap_err().code(),
            "constraint_violation",
            "{sql}"
        );
    }
    assert_eq!(count(&db, "SELECT total_ttc FROM documents"), 11_900);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM document_lines"), 1);

    // Rows of another draft cannot be moved into the validated document either. The draft's
    // line number and tax rate differ from the ticket's, so only the triggers can refuse.
    exec(
        &db,
        "INSERT INTO documents (id, doc_type, series, business_date, user_id, terminal_id,
                                cash_session_id, subtotal_ht, tax_total, total_ttc, created_at, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000d002', 'ticket', 'C1', '2026-09-28', 'u1', 't1',
                   '00000000-0000-7000-8000-00000000c001', 1000, 90, 1090, 0, 0);
         INSERT INTO document_lines (id, document_id, line_no, description, qty,
                                     unit_price, tax_rate_bp, total_ht, tax_amount, total_ttc)
           VALUES ('00000000-0000-7000-8000-00000000e003', '00000000-0000-7000-8000-00000000d002', 2,
                   'Yaourt', 1000, 1090, 900, 1000, 90, 1090);
         INSERT INTO document_taxes (document_id, rate_bp, base_ht, tax_amount)
           VALUES ('00000000-0000-7000-8000-00000000d002', 900, 1000, 90);
         INSERT INTO payments (id, document_id, cash_session_id, method, amount, created_at, user_id, terminal_id)
           VALUES ('00000000-0000-7000-8000-00000000f004', '00000000-0000-7000-8000-00000000d002',
                   '00000000-0000-7000-8000-00000000c001', 'cash', 1090, 0, 'u1', 't1');",
    )
    .unwrap();
    for sql in [
        "UPDATE document_lines SET document_id = '00000000-0000-7000-8000-00000000d001'
           WHERE id = '00000000-0000-7000-8000-00000000e003'",
        "UPDATE document_taxes SET document_id = '00000000-0000-7000-8000-00000000d001'
           WHERE document_id = '00000000-0000-7000-8000-00000000d002'",
        "UPDATE payments SET document_id = '00000000-0000-7000-8000-00000000d001'
           WHERE id = '00000000-0000-7000-8000-00000000f004'",
    ] {
        assert_eq!(
            exec(&db, sql).unwrap_err().code(),
            "constraint_violation",
            "{sql}"
        );
    }
    let validated = "SELECT (SELECT COUNT(*) FROM document_lines WHERE document_id = '00000000-0000-7000-8000-00000000d001')
         || '/' || (SELECT COUNT(*) FROM document_taxes WHERE document_id = '00000000-0000-7000-8000-00000000d001')
         || '/' || (SELECT COUNT(*) FROM payments WHERE document_id = '00000000-0000-7000-8000-00000000d001')
         || '/' || (SELECT SUM(amount) FROM payments WHERE document_id = '00000000-0000-7000-8000-00000000d001')";
    let owned: String = db
        .read(move |c| Ok(c.query_row(validated, [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(
        owned, "1/1/1/11900",
        "lines/taxes/payments/paid of the ticket"
    );
    // The other draft's rows stay its own and remain editable.
    exec(
        &db,
        "UPDATE document_lines SET description = 'Yaourt nature'
           WHERE id = '00000000-0000-7000-8000-00000000e003';
         UPDATE document_taxes SET base_ht = 1000
           WHERE document_id = '00000000-0000-7000-8000-00000000d002';
         UPDATE payments SET amount = 1090 WHERE id = '00000000-0000-7000-8000-00000000f004';",
    )
    .unwrap();
}

#[test]
fn documents_are_born_drafts_and_validation_needs_number_and_hash() {
    let db = seeded();
    let born_validated = "INSERT INTO documents (id, doc_type, status, number, fiscal_year, business_date,
        user_id, terminal_id, validated_at, chain_seq, prev_hash, hash, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000d002', 'ticket', 'validated', 'TK2026-C1-000009', 2026,
        '2026-09-28', 'u1', 't1', 1, 1,
        '0000000000000000000000000000000000000000000000000000000000000000',
        'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 0, 0)";
    assert_eq!(
        exec(&db, born_validated).unwrap_err().code(),
        "constraint_violation"
    );
    let without_hash = "UPDATE documents SET status = 'validated', number = 'TK2026-C1-000001',
        fiscal_year = 2026, validated_at = 1, chain_seq = 1";
    assert_eq!(
        exec(&db, without_hash).unwrap_err().code(),
        "constraint_violation"
    );
    let unbalanced = "UPDATE documents SET total_ttc = 11901";
    assert_eq!(
        exec(&db, unbalanced).unwrap_err().code(),
        "constraint_violation"
    );
    // A cheque tender without its details is refused (timbre exemption needs them).
    let bare_cheque =
        "INSERT INTO payments (id, document_id, method, amount, created_at, user_id, terminal_id)
      VALUES ('00000000-0000-7000-8000-00000000f003', '00000000-0000-7000-8000-00000000d001',
              'cheque', 100, 0, 'u1', 't1')";
    assert_eq!(
        exec(&db, bare_cheque).unwrap_err().code(),
        "constraint_violation"
    );
    // Cancelling a draft is allowed; it is final afterwards.
    exec(&db, "UPDATE documents SET status = 'cancelled'").unwrap();
    assert_eq!(
        exec(&db, "UPDATE documents SET status = 'draft'")
            .unwrap_err()
            .code(),
        "constraint_violation"
    );
}

#[test]
fn one_open_session_per_terminal_and_closed_sessions_are_final() {
    let db = seeded();
    let second = "INSERT INTO cash_sessions (id, terminal_id, user_id, business_date, opened_at, opening_float)
      VALUES ('00000000-0000-7000-8000-00000000c002', 't1', 'u2', '2026-09-28', 5, 0)";
    assert_eq!(
        exec(&db, second).unwrap_err().code(),
        "constraint_violation"
    );
    exec(
        &db,
        "INSERT INTO cash_movements (id, session_id, kind, amount, created_at, user_id)
           VALUES ('00000000-0000-7000-8000-00000000c101', '00000000-0000-7000-8000-00000000c001', 'paid_in', 500, 1, 'u1');",
    )
    .unwrap();
    let wrong_sign = "INSERT INTO cash_movements (id, session_id, kind, amount, created_at, user_id)
      VALUES ('00000000-0000-7000-8000-00000000c102', '00000000-0000-7000-8000-00000000c001', 'paid_out', 500, 1, 'u1')";
    assert_eq!(
        exec(&db, wrong_sign).unwrap_err().code(),
        "constraint_violation"
    );
    exec(
        &db,
        "UPDATE cash_sessions SET status = 'closed', closed_at = 9, closed_by = 'u1', expected_cash = 500,
           counted_cash = 500, variance = 0, z_number = 'ZC1-000001'",
    )
    .unwrap();
    for sql in [
        "UPDATE cash_sessions SET status = 'open'",
        "UPDATE cash_sessions SET notes = 'edited'",
        "DELETE FROM cash_sessions",
        "INSERT INTO cash_movements (id, session_id, kind, amount, created_at, user_id)
           VALUES ('00000000-0000-7000-8000-00000000c103', '00000000-0000-7000-8000-00000000c001', 'paid_in', 5, 10, 'u1')",
        "UPDATE cash_movements SET amount = 1",
        "DELETE FROM cash_movements",
    ] {
        assert_eq!(exec(&db, sql).unwrap_err().code(), "constraint_violation", "{sql}");
    }
    // A new session can open once the previous one is closed.
    exec(&db, second).unwrap();
}

#[test]
fn product_search_index_follows_the_products_table() {
    let db = seeded();
    let hits = |db: &Db, q: &'static str| -> i64 {
        db.read(move |c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM products_fts WHERE products_fts MATCH ?1",
                [q],
                |r| r.get(0),
            )?)
        })
        .unwrap()
    };
    exec(
        &db,
        "UPDATE products SET search_text = 'حليب lait candia', reference = 'LT-1'",
    )
    .unwrap();
    assert_eq!(hits(&db, "\"cand\"*"), 1);
    assert_eq!(hits(&db, "\"حل\"*"), 1);
    exec(&db, "UPDATE products SET search_text = 'yaourt'").unwrap();
    assert_eq!(hits(&db, "\"cand\"*"), 0);
    assert_eq!(hits(&db, "\"yaou\"*"), 1);
    exec(
        &db,
        "INSERT INTO products_fts (products_fts, rank) VALUES ('integrity-check', 1)",
    )
    .unwrap();
}

/// Every foreign key of every table leads an index (or the primary key), so that deletes and
/// joins never scan.
#[test]
fn every_foreign_key_is_indexed() {
    let db = Db::open_memory().unwrap();
    let missing: Vec<String> = db
        .read(|c| {
            let mut tables = c.prepare(
                "SELECT name FROM pragma_table_list WHERE schema = 'main' AND type = 'table'
                 AND name NOT LIKE 'sqlite_%'",
            )?;
            let tables = tables
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            let mut missing = Vec::new();
            for table in tables {
                let mut fks = c.prepare("SELECT \"from\" FROM pragma_foreign_key_list(?1)")?;
                let fks = fks
                    .query_map([&table], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                // An expression (`idx_documents_recent`) has no column name: it leads no key.
                let mut leads = c.prepare(
                    "SELECT ii.name FROM pragma_index_list(?1) il, pragma_index_info(il.name) ii
                     WHERE ii.seqno = 0 AND ii.name IS NOT NULL",
                )?;
                let leads = leads
                    .query_map([&table], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                for fk in fks {
                    if !leads.contains(&fk) {
                        missing.push(format!("{table}.{fk}"));
                    }
                }
            }
            Ok(missing)
        })
        .unwrap();
    assert!(
        missing.is_empty(),
        "foreign keys without an index: {missing:?}"
    );
}

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/m0-foundation.db")
}

/// Regenerates the M0 fixture: migration 0001 only, a company, the system roles, the local
/// terminal, an owner and two audited writes, as a shop left it at the end of M0.
#[test]
#[ignore = "writes tests/fixtures/m0-foundation.db; run once, then commit the file"]
fn write_m0_fixture() {
    let path = fixture_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);
    let mut conn = Connection::open(&path).unwrap();
    let only_m0 = [rusqlite_migration::M::up(include_str!(
        "../../migrations/0001_foundation.sql"
    ))];
    rusqlite_migration::Migrations::from_slice(&only_m0)
        .to_latest(&mut conn)
        .unwrap();
    let tx = conn.transaction().unwrap();
    crate::auth::roles::seed_system_roles(&tx).unwrap();
    let terminal = crate::terminal::ensure_local(&tx).unwrap();
    let pack = crate::datapack::bundled().unwrap();
    let actor = Actor::system(Some(terminal.id.clone()));
    company::save(
        &tx,
        &actor,
        &pack,
        CompanyInput::named(Lang::Fr, "Supérette El Amel"),
    )
    .unwrap();
    tx.execute(
        "INSERT INTO users (id, username, display_name, pin_hash, role_id, created_at, updated_at)
         VALUES ('00000000-0000-7000-8000-0000000a0001', 'amine', 'Amine', 'fixture-hash',
                 '00000000-0000-7000-8000-000000000001', 0, 0)",
        [],
    )
    .unwrap();
    audit::append(
        &tx,
        &actor.event("setup.completed", "company", Some("1"), json!({})),
    )
    .unwrap();
    tx.commit().unwrap();
    meta_set(&conn, META_CLEAN_SHUTDOWN, "1").unwrap();
    assert_eq!(schema_version(&conn).unwrap(), 1);
}

#[test]
fn the_m0_fixture_upgrades_in_place_after_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    std::fs::copy(fixture_path(), &path).unwrap();
    let db = Db::open(&path, DbOptions::default()).unwrap();
    let report = db.report().clone();
    assert_eq!((report.schema_version, report.migrations_applied), (10, 9));
    assert!(!report.unclean_shutdown);
    // pre_migration_backup_is_taken (owed by M0): the copy is the untouched M0 database.
    let backup = report
        .pre_migration_backup
        .expect("a backup before migrating");
    let copy = Connection::open(&backup).unwrap();
    assert_eq!(schema_version(&copy).unwrap(), 1);
    assert!(integrity_ok(&copy).unwrap());
    drop(copy);
    // Nothing of M0 was lost, and the new schema is complete and consistent.
    assert!(db.integrity_check().unwrap());
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM pragma_foreign_key_check"),
        0
    );
    let company = db.read(company::get).unwrap().unwrap();
    assert_eq!(company.name_fr, "Supérette El Amel");
    assert_eq!(count(&db, "SELECT COUNT(*) FROM users"), 1);
    let chain = db.read(audit::verify_chain).unwrap();
    assert_eq!((chain.rows, chain.first_broken_seq), (2, None));
    assert_eq!(count(&db, "SELECT COUNT(*) FROM units"), 14);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM price_levels"), 4);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM stock_locations"), 2);
}

#[test]
fn checkpoint_copies_the_wal_into_the_database_file() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("pos.db"), DbOptions::default()).unwrap();
    db.write(|tx| meta_set(tx, "probe", "x")).unwrap();
    let (frames, copied) = db.checkpoint().unwrap();
    assert!(frames > 0 && copied == frames, "{frames} {copied}");
    assert_eq!(Db::open_memory().unwrap().checkpoint().unwrap(), (0, 0));
}
