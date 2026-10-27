//! Migration 0009: per-lot count lines (STK-004) and the
//! payment status of online orders, and the upgrade of a schema-8 database.

use super::*;
use crate::catalog::extras::{self, ProductExtrasInput};
use crate::company::{PriceMode, TaxRegime};
use crate::money::{Money, Qty};
use crate::stock::lots::LotInput;
use crate::stock::{ReceiveLine, SHOP_LOCATION_ID, counts, receive};
use std::path::PathBuf;

/// The schema the pre-0009 fixture was written at: main after the M8 engine merge (0001–0008).
const PRE_0009_SCHEMA: i64 = 8;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pre-0009-shop.db")
}

/// Writes the pre-0009 fixture: a shop at schema 8 with a lot-tracked medicine (lots L-A 10 and
/// L-B 5, 2 more without a lot), an open count of it and one online order. Run once, before
/// 0009 is listed.
#[test]
#[ignore = "writes tests/fixtures/pre-0009-shop.db once, before 0009 is listed"]
fn write_pre_0009_fixture() {
    let path = fixture_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);
    let db = Db::open(&path, DbOptions::default()).unwrap();
    assert_eq!(
        db.report().schema_version,
        PRE_0009_SCHEMA,
        "write the pre-0009 fixture before listing 0009"
    );
    let shop = crate::testkit::shop_in(db, TaxRegime::Reel, PriceMode::Ttc);
    let doliprane = shop.product("Doliprane 500", Money(25_000), &shop.tva9).id;
    let (actor, id) = (shop.owner.actor(), doliprane.clone());
    shop.db
        .write(move |tx| {
            extras::set(
                tx,
                &actor,
                &id,
                ProductExtrasInput {
                    track_lots: true,
                    ..ProductExtrasInput::default()
                },
            )
        })
        .unwrap();
    for (lot, expiry, qty) in [("L-A", "2027-01-31", 10_000), ("L-B", "2027-06-30", 5_000)] {
        let line = ReceiveLine {
            product_id: doliprane.clone(),
            location_id: SHOP_LOCATION_ID.into(),
            qty: Qty(qty),
            unit_cost: Money(18_000),
            lot: Some(LotInput {
                lot_number: lot.into(),
                expiry_date: Some(expiry.into()),
            }),
            serials: Vec::new(),
            supplier_id: None,
            ref_type: "document".into(),
            ref_id: "rc".into(),
            note: None,
        };
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| receive(tx, &actor, &line, 1))
            .unwrap();
    }
    shop.stock_in(&doliprane, 2_000);
    let (ctx, id) = (shop.owner_till(), doliprane.clone());
    shop.db
        .write(move |tx| {
            counts::start(
                tx,
                &ctx,
                counts::CountInput {
                    location_id: None,
                    scope: counts::CountScope::Products,
                    category_id: None,
                    product_ids: vec![id],
                    blind: false,
                    note: None,
                },
            )
        })
        .unwrap();
    shop.db
        .write(|tx| {
            tx.execute_batch(
                "INSERT INTO online_orders (id, store_id, remote_id, order_number, remote_status, payment_method,
                     delivery_type, customer_name, subtotal, shipping_cost, discount, total, local_state,
                     remote_created_at, remote_updated_at, created_at, updated_at)
                 VALUES ('00000000-0000-7000-8000-00000000c901', 'store-1', '9001', '1042', 'confirmed', 'cod',
                         'pickup', 'Samira B.', 20000, 0, 0, 20000, 'new', 1, 1, 1, 1);",
            )?;
            Ok(())
        })
        .unwrap();
    shop.db.clone().close().unwrap();
}

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

/// Two medicines with lots (L-A and L-B of Doliprane, L-C of Spasfon) and an open count holding
/// Doliprane's line with one lot line (ids end in `…e0xx`).
const LOT_ROWS: &str = "
    INSERT INTO tax_rates (id, code, rate_bp, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000e401', 'TVA9', 900, 0, 0);
    INSERT INTO products (id, name_fr, unit_id, tax_rate_id, sell_price, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000e001', 'Doliprane 500', '00000000-0000-7000-8000-000000000101',
              '00000000-0000-7000-8000-00000000e401', 25000, 0, 0),
             ('00000000-0000-7000-8000-00000000e002', 'Spasfon', '00000000-0000-7000-8000-000000000101',
              '00000000-0000-7000-8000-00000000e401', 32000, 0, 0);
    INSERT INTO lots (id, product_id, lot_number, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000e101', '00000000-0000-7000-8000-00000000e001', 'L-A', 0, 0),
             ('00000000-0000-7000-8000-00000000e102', '00000000-0000-7000-8000-00000000e001', 'L-B', 0, 0),
             ('00000000-0000-7000-8000-00000000e103', '00000000-0000-7000-8000-00000000e002', 'L-C', 0, 0);
    INSERT INTO stock_counts (id, location_id, scope, status, started_at, user_id)
      VALUES ('00000000-0000-7000-8000-00000000e301', '00000000-0000-7000-8000-000000000301', 'products',
              'open', 1, 'u1');
    INSERT INTO stock_count_lines (count_id, product_id, expected_qty)
      VALUES ('00000000-0000-7000-8000-00000000e301', '00000000-0000-7000-8000-00000000e001', 15000);
    INSERT INTO stock_count_lot_lines (count_id, product_id, lot_id, expected_qty, counted_qty, counted_at, counted_by)
      VALUES ('00000000-0000-7000-8000-00000000e301', '00000000-0000-7000-8000-00000000e001',
              '00000000-0000-7000-8000-00000000e101', 10000, 8000, 2, 'u1');";

#[test]
fn per_lot_count_lines_belong_to_their_product_line_and_freeze_with_the_count() {
    let db = Db::open_memory().unwrap();
    assert_eq!(db.report().schema_version, 10);
    exec(&db, LOT_ROWS).unwrap();
    let refused = [
        (
            "a lot of another product",
            "INSERT INTO stock_count_lot_lines (count_id, product_id, lot_id, expected_qty, counted_qty, counted_at)
               VALUES ('00000000-0000-7000-8000-00000000e301', '00000000-0000-7000-8000-00000000e001',
                       '00000000-0000-7000-8000-00000000e103', 0, 1000, 2)",
        ),
        (
            "a lot line without its product line",
            "INSERT INTO stock_count_lot_lines (count_id, product_id, lot_id, expected_qty, counted_qty, counted_at)
               VALUES ('00000000-0000-7000-8000-00000000e301', '00000000-0000-7000-8000-00000000e002',
                       '00000000-0000-7000-8000-00000000e103', 0, 1000, 2)",
        ),
        (
            "never a negative count",
            "INSERT INTO stock_count_lot_lines (count_id, product_id, lot_id, expected_qty, counted_qty, counted_at)
               VALUES ('00000000-0000-7000-8000-00000000e301', '00000000-0000-7000-8000-00000000e001',
                       '00000000-0000-7000-8000-00000000e102', 5000, -1, 2)",
        ),
    ];
    for (why, sql) in refused {
        assert!(exec(&db, sql).is_err(), "{why}");
    }
    exec(
        &db,
        "UPDATE stock_count_lot_lines SET counted_qty = 9000;
         INSERT INTO stock_count_lot_lines (count_id, product_id, lot_id, expected_qty, counted_qty, counted_at)
           VALUES ('00000000-0000-7000-8000-00000000e301', '00000000-0000-7000-8000-00000000e001',
                   '00000000-0000-7000-8000-00000000e102', 5000, 6000, 3);
         DELETE FROM stock_count_lot_lines WHERE lot_id = '00000000-0000-7000-8000-00000000e102';",
    )
    .unwrap();
    exec(
        &db,
        "UPDATE stock_counts SET status = 'closed', closed_at = 5, closed_by = 'u1'",
    )
    .unwrap();
    for sql in [
        "UPDATE stock_count_lot_lines SET posted_delta = 1",
        "DELETE FROM stock_count_lot_lines",
        "INSERT INTO stock_count_lot_lines (count_id, product_id, lot_id, expected_qty, counted_qty, counted_at)
           VALUES ('00000000-0000-7000-8000-00000000e301', '00000000-0000-7000-8000-00000000e001',
                   '00000000-0000-7000-8000-00000000e102', 5000, 6000, 3)",
    ] {
        let err = db.write(move |tx| Ok(tx.execute_batch(sql)?)).unwrap_err();
        assert_eq!(err.code(), "constraint_violation", "{sql}");
    }
    assert_eq!(
        count(&db, "SELECT counted_qty FROM stock_count_lot_lines"),
        9_000
    );
}

#[test]
fn an_online_order_keeps_the_payment_status_dzbuild_sends() {
    let db = Db::open_memory().unwrap();
    let column: (String, i64) = db
        .read(|c| {
            Ok(c.query_row(
                "SELECT type, \"notnull\" FROM pragma_table_info('online_orders') WHERE name = 'payment_status'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .unwrap();
    assert_eq!(
        column,
        ("TEXT".to_owned(), 0),
        "nullable: the orders mirrored before 0009 have none"
    );
}

/// The fixture's open count is finished lot by lot after the upgrade: L-A 8 and L-B 6 on the
/// shelf; the 2 pieces the books held without a lot were not there.
#[test]
fn the_pre_0009_fixture_upgrades_in_place_after_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    std::fs::copy(fixture_path(), &path).unwrap();
    let db = Db::open(&path, DbOptions::default()).unwrap();
    let report = db.report().clone();
    assert_eq!((report.schema_version, report.migrations_applied), (10, 2));
    let backup = report
        .pre_migration_backup
        .expect("a backup before migrating");
    let copy = rusqlite::Connection::open(&backup).unwrap();
    assert_eq!(schema_version(&copy).unwrap(), PRE_0009_SCHEMA);
    assert!(integrity_ok(&copy).unwrap());
    drop(copy);
    assert!(db.integrity_check().unwrap());
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM pragma_foreign_key_check"),
        0
    );
    assert_eq!(
        db.read(crate::audit::verify_chain)
            .unwrap()
            .first_broken_seq,
        None
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM online_orders WHERE remote_id = '9001' AND payment_status IS NULL"
        ),
        1
    );
    let (owner_id, count_id, product_id): (String, String, String) = db
        .read(|c| {
            Ok(c.query_row(
                "SELECT (SELECT id FROM users WHERE role_id = ?1),
                        (SELECT id FROM stock_counts WHERE status = 'open'),
                        (SELECT id FROM products WHERE name_fr = 'Doliprane 500')",
                [crate::auth::OWNER_ROLE_ID],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?)
        })
        .unwrap();
    let terminal = db.write(|tx| crate::terminal::ensure_local(tx)).unwrap();
    let session = db
        .read(move |c| {
            crate::auth::session::build_session(c, &owner_id, &terminal, crate::testkit::NOON)
        })
        .unwrap();
    let ctx = crate::till::TillContext::new(
        session,
        std::sync::Arc::new(crate::datapack::bundled().unwrap()),
        crate::testkit::NOON,
    );
    for (lot, qty) in [("L-A", 8_000), ("L-B", 6_000)] {
        let (ctx, id, product) = (ctx.clone(), count_id.clone(), product_id.clone());
        db.write(move |tx| {
            counts::set_counted_lot(
                tx,
                &ctx,
                &id,
                &product,
                lot,
                Qty(qty),
                counts::CountMode::Set,
            )
        })
        .unwrap();
    }
    let (c, id) = (ctx.clone(), count_id.clone());
    db.write(move |tx| counts::close(tx, &c, &id, counts::Uncounted::Ignore))
        .unwrap();
    let levels: String = db
        .read(|c| {
            Ok(c.query_row(
                "SELECT group_concat(n || '=' || q, ' ') FROM (
                   SELECT l.lot_number AS n, ll.qty AS q FROM lot_levels ll JOIN lots l ON l.id = ll.lot_id
                   ORDER BY l.lot_number)",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(levels, "L-A=8000 L-B=6000");
    assert_eq!(count(&db, "SELECT SUM(qty) FROM stock_levels"), 14_000);
    assert!(db.integrity_check().unwrap());
}
