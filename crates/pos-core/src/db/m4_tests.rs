//! Migration 0005 (hardware): scale items and the last label printed per product, and an M1
//! database climbing every migration since (M3's 0004, then 0005).

use super::*;
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

const TOMATO: &str = "00000000-0000-7000-8000-00000000f001";

const PRODUCTS: &str = "
    INSERT INTO tax_rates (id, code, rate_bp, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000f401', 'TVA9', 900, 0, 0);
    INSERT INTO products (id, name_fr, unit_id, tax_rate_id, sell_price, is_weighed, plu, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000f001', 'Tomates', '00000000-0000-7000-8000-000000000102',
              '00000000-0000-7000-8000-00000000f401', 12000, 1, 10012, 0, 0),
             ('00000000-0000-7000-8000-00000000f002', 'Courgettes', '00000000-0000-7000-8000-000000000102',
              '00000000-0000-7000-8000-00000000f401', 9000, 1, 10013, 0, 0);";

#[test]
fn scale_items_check_their_values_and_hotkeys_are_unique() {
    let db = Db::open_memory().unwrap();
    assert_eq!(db.report().schema_version, migrations::LATEST_VERSION);
    exec(&db, PRODUCTS).unwrap();
    exec(
        &db,
        "INSERT INTO scale_items (product_id, hotkey, tare_g, shelf_life_days, updated_at)
         VALUES ('00000000-0000-7000-8000-00000000f001', 12, 150, 3, 0)",
    )
    .unwrap();
    for refused in [
        // the hotkey is taken by the tomatoes
        "INSERT INTO scale_items (product_id, hotkey, updated_at)
         VALUES ('00000000-0000-7000-8000-00000000f002', 12, 0)",
        "INSERT INTO scale_items (product_id, hotkey, updated_at)
         VALUES ('00000000-0000-7000-8000-00000000f002', 0, 0)",
        "INSERT INTO scale_items (product_id, tare_g, updated_at)
         VALUES ('00000000-0000-7000-8000-00000000f002', 100000, 0)",
        "INSERT INTO scale_items (product_id, shelf_life_days, updated_at)
         VALUES ('00000000-0000-7000-8000-00000000f002', 4000, 0)",
        // no such product
        "INSERT INTO scale_items (product_id, updated_at)
         VALUES ('00000000-0000-7000-8000-00000000f999', 0)",
    ] {
        assert!(exec(&db, refused).is_err(), "{refused}");
    }
    exec(
        &db,
        "INSERT INTO scale_items (product_id, hotkey, updated_at)
         VALUES ('00000000-0000-7000-8000-00000000f002', NULL, 0)",
    )
    .unwrap();
    assert_eq!(count(&db, "SELECT COUNT(*) FROM scale_items"), 2);
}

#[test]
fn label_prints_keep_one_row_per_product() {
    let db = Db::open_memory().unwrap();
    exec(&db, PRODUCTS).unwrap();
    let upsert = |price: i64, copies: i64| {
        db.write(move |tx| {
            tx.execute(
                "INSERT INTO label_prints (product_id, printed_at, template_id, lang, price, name, copies)
                 VALUES (?1, 1, 'builtin.price_40x25', 'ar_fr', ?2, 'Tomates', ?3)
                 ON CONFLICT (product_id) DO UPDATE
                   SET price = excluded.price, copies = excluded.copies",
                rusqlite::params![TOMATO, price, copies],
            )?;
            Ok(())
        })
    };
    upsert(12_000, 3).unwrap();
    upsert(13_000, 0).unwrap(); // a dismissed stale label keeps copies 0
    assert_eq!(count(&db, "SELECT COUNT(*) FROM label_prints"), 1);
    assert_eq!(count(&db, "SELECT price FROM label_prints"), 13_000);
    assert!(upsert(-1, 1).is_err(), "prices are never negative");
    assert!(upsert(100, -1).is_err(), "copies are never negative");
    assert!(
        exec(
            &db,
            "INSERT INTO label_prints (product_id, printed_at, template_id, lang, price, name, copies)
             VALUES ('00000000-0000-7000-8000-00000000f002', 1, 'x', 'de', 0, 'Courgettes', 1)"
        )
        .is_err(),
        "only the six label languages"
    );
}

/// The M1 fixture (schema 2) climbs 0003 and every later migration after a pre-migration backup,
/// and keeps its validated ticket and audit chain.
#[test]
fn an_m1_database_climbs_every_migration_and_keeps_its_ticket() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/m1-first-sale.db"),
        &path,
    )
    .unwrap();
    let db = Db::open(&path, DbOptions::default()).unwrap();
    assert_eq!(db.report().schema_version, migrations::LATEST_VERSION);
    assert!(db.report().pre_migration_backup.is_some());
    assert!(db.integrity_check().unwrap());
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM documents WHERE status = 'validated'"
        ),
        1
    );
    assert_eq!(count(&db, "SELECT COUNT(*) FROM scale_items"), 0);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM label_prints"), 0);
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM pragma_foreign_key_check"),
        0
    );
    let chain = db.read(crate::audit::verify_chain).unwrap();
    assert_eq!(chain.first_broken_seq, None);
}
