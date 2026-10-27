//! Migration 0006 (DZBuild link): the change feed and its consumers, the cloud tables and their
//! guards, and the upgrade of an M1 database with a silent feed. `LIST` holds 0001-0006 in order
//! (M4's 0005 before this one) and later milestones append theirs, so the schema version is read
//! from `migrations::LATEST_VERSION`; `migrations_are_valid` pins the number.

use super::*;
use crate::audit;
use crate::documents;
use std::path::PathBuf;

const PRODUCT_ID: &str = "00000000-0000-7000-8000-00000000e001";

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

const PRODUCT: &str = "
    INSERT INTO tax_rates (id, code, rate_bp, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000e401', 'TVA19', 1900, 0, 0);
    INSERT INTO products (id, name_fr, unit_id, tax_rate_id, sell_price, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000e001', 'Lait', '00000000-0000-7000-8000-000000000101',
              '00000000-0000-7000-8000-00000000e401', 10000, 0, 0);";

const CONSUMER: &str = "INSERT INTO sync_cursors (name, kind, last_seq, updated_at)
    VALUES ('changelog:cloud', 'changelog', 0, 0);";

const LINK: &str =
    "INSERT INTO cloud_links (id, account_sub, store_id, store_name, status, linked_at)
    VALUES ('00000000-0000-7000-8000-00000000c001', '84211', '141', 'Boutique Amel', 'linked', 1);";

fn feed(db: &Db) -> Vec<(String, String, String, serde_json::Value)> {
    db.read(|c| {
        let mut stmt =
            c.prepare("SELECT table_name, row_id, op, payload FROM changelog ORDER BY seq")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .unwrap()
    .into_iter()
    .map(|(t, id, op, p)| (t, id, op, serde_json::from_str(&p).unwrap()))
    .collect()
}

#[test]
fn the_link_tables_exist() {
    let db = Db::open_memory().unwrap();
    assert_eq!(db.report().schema_version, migrations::LATEST_VERSION);
    for name in [
        "changelog",
        "sync_cursors",
        "cloud_links",
        "cloud_map",
        "sync_outbox",
        "cloud_stock",
        "cloud_variant_map",
        "cloud_publish",
        "cloud_inbox",
        "cloud_endpoints",
        "cloud_notices",
        "online_orders",
        "online_order_lines",
        "remote_customer_flags",
        "cloud_backup_keys",
        "cloud_backups",
    ] {
        assert!(table_exists(&db, name), "{name}");
    }
}

#[test]
fn the_change_feed_is_silent_until_a_changelog_consumer_registers() {
    let db = Db::open_memory().unwrap();
    exec(&db, PRODUCT).unwrap();
    exec(&db, "UPDATE products SET sell_price = 11000").unwrap();
    exec(
        &db,
        "INSERT INTO sync_cursors (name, kind, cursor, updated_at) VALUES ('cloud:events', 'remote', 'c1', 0)",
    )
    .unwrap();
    exec(&db, "UPDATE products SET sell_price = 11500").unwrap();
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM changelog"),
        0,
        "no changelog consumer, no feed"
    );

    exec(&db, CONSUMER).unwrap();
    exec(
        &db,
        "UPDATE products SET sell_price = 12000;
         INSERT INTO product_barcodes (id, product_id, barcode, pack_qty, created_at)
           VALUES ('00000000-0000-7000-8000-00000000e101', '00000000-0000-7000-8000-00000000e001', '6130000012345', 1000, 0);
         INSERT INTO stock_movements (id, product_id, location_id, qty_delta, reason, ref_type, ref_id, created_at)
           VALUES ('00000000-0000-7000-8000-00000000e201', '00000000-0000-7000-8000-00000000e001',
                   '00000000-0000-7000-8000-000000000301', 5000, 'opening', NULL, NULL, 1);
         INSERT INTO cloud_publish (product_id, publish, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000e001', 1, 1);",
    )
    .unwrap();
    let rows = feed(&db);
    let tables: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(
        tables,
        [
            "products",
            "product_barcodes",
            "stock_movements",
            "cloud_publish"
        ]
    );
    assert_eq!(rows[1].3["product_id"], PRODUCT_ID);
    assert_eq!(rows[2].3["product_id"], PRODUCT_ID);
    assert_eq!(rows[2].3["reason"], "opening");
    assert_eq!(rows[3].1, PRODUCT_ID, "publish rows are keyed by product");
    assert!(rows.iter().all(|r| r.2 == "insert" || r.2 == "update"));
}

#[test]
fn a_document_enters_the_feed_once_when_it_is_validated() {
    let db = Db::open_memory().unwrap();
    exec(&db, CONSUMER).unwrap();
    exec(
        &db,
        "INSERT INTO documents (id, doc_type, series, business_date, user_id, terminal_id, created_at, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000f001', 'ticket', 'C1', '2026-09-28', 'u', 't', 0, 0);
         UPDATE documents SET note = 'brouillon' WHERE id = '00000000-0000-7000-8000-00000000f001';",
    )
    .unwrap();
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM changelog"),
        0,
        "drafts are not replicated"
    );
    exec(
        &db,
        "UPDATE documents SET status = 'validated', number = 'TK2026-C1-000001', fiscal_year = 2026,
                validated_at = 1, chain_seq = 1, prev_hash = printf('%064d', 0), hash = printf('%064d', 1)
         WHERE id = '00000000-0000-7000-8000-00000000f001';",
    )
    .unwrap();
    let rows = feed(&db);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "documents");
    assert_eq!(rows[0].3["doc_type"], "ticket");
    assert!(rows[0].3["parent_id"].is_null());
}

#[test]
fn the_change_feed_is_append_only() {
    let db = Db::open_memory().unwrap();
    exec(&db, CONSUMER).unwrap();
    exec(&db, PRODUCT).unwrap();
    assert!(exec(&db, "UPDATE changelog SET op = 'delete'").is_err());
    exec(&db, "DELETE FROM changelog").unwrap(); // pruning is how readers let go of rows
    assert!(
        exec(
            &db,
            "INSERT INTO sync_cursors (name, kind, updated_at) VALUES ('changelog:lan:C2', 'changelog', 0)"
        )
        .is_err(),
        "a changelog consumer has a position"
    );
}

#[test]
fn one_active_link_at_a_time_and_links_are_kept() {
    let db = Db::open_memory().unwrap();
    exec(&db, LINK).unwrap();
    let second = exec(
        &db,
        "INSERT INTO cloud_links (id, account_sub, store_id, store_name, status, linked_at)
         VALUES ('00000000-0000-7000-8000-00000000c002', '84211', '152', 'Amel Kids', 'linked', 2)",
    );
    assert!(second.is_err(), "one active link");
    assert!(
        exec(&db, "UPDATE cloud_links SET status = 'unlinked'").is_err(),
        "an unlinked link says when"
    );
    exec(
        &db,
        "UPDATE cloud_links SET status = 'unlinked', unlinked_at = 3",
    )
    .unwrap();
    exec(
        &db,
        "INSERT INTO cloud_links (id, account_sub, store_id, store_name, status, linked_at)
         VALUES ('00000000-0000-7000-8000-00000000c002', '84211', '152', 'Amel Kids', 'relink_required', 4)",
    )
    .unwrap();
    assert!(exec(&db, "DELETE FROM cloud_links").is_err());
    assert!(
        exec(
            &db,
            "UPDATE cloud_links SET scopes = '{\"a\":1}' WHERE store_id = '152'"
        )
        .is_err(),
        "scopes are a JSON array"
    );
}

#[test]
fn outbox_keys_are_short_safe_and_frozen_once_attempted() {
    let db = Db::open_memory().unwrap();
    let insert = |id: &'static str, key: &'static str| {
        let sql: &'static str = Box::leak(
            format!(
                "INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, idempotency_key, created_at)
                 VALUES ('{id}', 'cloud', '141', 'pos_sale', 'd1', '{key}', 0)"
            )
            .into_boxed_str(),
        );
        exec(&db, sql)
    };
    insert(
        "00000000-0000-7000-8000-00000000a001",
        "sale:00000000-0000-7000-8000-00000000f001",
    )
    .unwrap();
    assert!(insert("00000000-0000-7000-8000-00000000a002", "sale with space").is_err());
    assert!(
        insert(
            "00000000-0000-7000-8000-00000000a003",
            "k0123456789012345678901234567890123456789012345678901234567890123"
        )
        .is_err(),
        "65 characters"
    );
    assert!(
        insert(
            "00000000-0000-7000-8000-00000000a004",
            "sale:00000000-0000-7000-8000-00000000f001"
        )
        .is_err(),
        "a key is used once"
    );
    exec(&db, "UPDATE sync_outbox SET attempts = 1").unwrap();
    assert!(exec(&db, "UPDATE sync_outbox SET idempotency_key = 'sale:other'").is_err());
    assert!(exec(&db, "UPDATE sync_outbox SET payload = '{\"x\":1}'").is_err());
    assert!(
        exec(&db, "DELETE FROM sync_outbox").is_err(),
        "pending items are kept"
    );
    assert!(
        exec(&db, "UPDATE sync_outbox SET status = 'sent'").is_err(),
        "sent says when"
    );
    exec(&db, "UPDATE sync_outbox SET status = 'sent', sent_at = 5").unwrap();
    exec(&db, "DELETE FROM sync_outbox").unwrap();
}

#[test]
fn product_upserts_coalesce_while_not_attempted() {
    let db = Db::open_memory().unwrap();
    exec(
        &db,
        "INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, idempotency_key, created_at)
         VALUES ('00000000-0000-7000-8000-00000000a011', 'cloud', '141', 'product_upsert', 'p1', 'product:a011', 0)",
    )
    .unwrap();
    let twin = "INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, idempotency_key, created_at)
         VALUES ('00000000-0000-7000-8000-00000000a012', 'cloud', '141', 'product_upsert', 'p1', 'product:a012', 0)";
    assert!(exec(&db, twin).is_err(), "one waiting upsert per product");
    exec(&db, "UPDATE sync_outbox SET attempts = 1").unwrap();
    exec(&db, twin).unwrap();
}

const DAY_MS: i64 = 86_400_000;

/// The plan's `outbox::prune_sent` (Task 7), statement and cutoff verbatim.
fn prune_sent(db: &Db, now: i64) -> Result<usize, AppError> {
    db.write(move |tx| {
        Ok(tx.execute(
            "DELETE FROM sync_outbox WHERE status IN ('sent', 'skipped') AND COALESCE(sent_at, created_at) < ?1",
            [now - 30 * DAY_MS],
        )?)
    })
}

/// `(status, depends_on, idempotency_key, payload, attempts, last_error)` of one outbox row.
type OutboxRow = (String, Option<String>, String, String, i64, Option<String>);

fn outbox_row(db: &Db, id: &'static str) -> OutboxRow {
    db.read(move |c| {
        Ok(c.query_row(
            "SELECT status, depends_on, idempotency_key, payload, attempts, last_error
             FROM sync_outbox WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )?)
    })
    .unwrap()
}

#[test]
fn pruning_a_sent_sale_releases_its_refunds_and_keeps_them() {
    // Day 40. The sale was sent on day 9 (31 days ago); its refunds: one attempted and still
    // pending, one sent on day 30 (10 days ago), one refused (failed).
    let db = Db::open_memory().unwrap();
    exec(
        &db,
        "INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, idempotency_key, status, attempts,
                                  created_at, sent_at)
           VALUES ('00000000-0000-7000-8000-00000000a021', 'cloud', '141', 'pos_sale', 'd1', 'sale:d1', 'sent', 1,
                   777600000, 777600000);
         INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, payload, idempotency_key, attempts,
                                  depends_on, created_at)
           VALUES ('00000000-0000-7000-8000-00000000a022', 'cloud', '141', 'pos_refund', 'r1', '{\"sale\":\"d1\"}',
                   'refund:r1', 1, '00000000-0000-7000-8000-00000000a021', 777600001);
         INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, idempotency_key, status, attempts,
                                  depends_on, created_at, sent_at)
           VALUES ('00000000-0000-7000-8000-00000000a023', 'cloud', '141', 'pos_refund', 'r2', 'refund:r2', 'sent', 1,
                   '00000000-0000-7000-8000-00000000a021', 777600002, 2592000000);
         INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, idempotency_key, status, attempts,
                                  last_error, depends_on, created_at)
           VALUES ('00000000-0000-7000-8000-00000000a024', 'cloud', '141', 'pos_refund', 'r3', 'refund:r3', 'failed', 1,
                   'validation_failed', '00000000-0000-7000-8000-00000000a021', 777600003);",
    )
    .unwrap();
    assert_eq!(
        prune_sent(&db, 40 * DAY_MS).unwrap(),
        1,
        "the sale is pruned although refunds still point at it"
    );
    assert_eq!(
        outbox_row(&db, "00000000-0000-7000-8000-00000000a022"),
        (
            "pending".to_owned(),
            None,
            "refund:r1".to_owned(),
            "{\"sale\":\"d1\"}".to_owned(),
            1,
            None
        ),
        "a sent sale satisfied the dependency; the attempted refund keeps its key and payload"
    );
    assert_eq!(
        outbox_row(&db, "00000000-0000-7000-8000-00000000a023").1,
        None
    );
    assert_eq!(
        outbox_row(&db, "00000000-0000-7000-8000-00000000a024").1,
        None
    );
    assert_eq!(prune_sent(&db, 61 * DAY_MS).unwrap(), 1, "the sent refund");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM sync_outbox"),
        2,
        "pending and failed refunds are kept"
    );
}

#[test]
fn a_skipped_sale_takes_its_open_refunds_with_it() {
    // A sale DZBuild will never receive: a refund of it can never be sent, so it must neither
    // wait forever nor be released (and sent) when the skipped sale is pruned 30 days on.
    let db = Db::open_memory().unwrap();
    exec(
        &db,
        "INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, idempotency_key, status, attempts,
                                  last_error, created_at)
           VALUES ('00000000-0000-7000-8000-00000000a031', 'cloud', '141', 'pos_sale', 'd1', 'sale:d1', 'failed', 1,
                   'validation_failed', 0);
         INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, payload, idempotency_key, depends_on,
                                  created_at)
           VALUES ('00000000-0000-7000-8000-00000000a032', 'cloud', '141', 'pos_refund', 'r1', '{\"sale\":\"d1\"}',
                   'refund:r1', '00000000-0000-7000-8000-00000000a031', 1);
         INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, idempotency_key, status, attempts,
                                  last_error, depends_on, created_at)
           VALUES ('00000000-0000-7000-8000-00000000a033', 'cloud', '141', 'pos_refund', 'r2', 'refund:r2', 'failed', 1,
                   'validation_failed', '00000000-0000-7000-8000-00000000a031', 2);
         INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, idempotency_key, created_at)
           VALUES ('00000000-0000-7000-8000-00000000a034', 'cloud', '141', 'pos_sale', 'd2', 'sale:d2', 3);",
    )
    .unwrap();
    exec(
        &db,
        "UPDATE sync_outbox SET status = 'skipped', last_error = 'dropped'
         WHERE id = '00000000-0000-7000-8000-00000000a031'",
    )
    .unwrap();
    assert_eq!(
        outbox_row(&db, "00000000-0000-7000-8000-00000000a032"),
        (
            "skipped".to_owned(),
            Some("00000000-0000-7000-8000-00000000a031".to_owned()),
            "refund:r1".to_owned(),
            "{\"sale\":\"d1\"}".to_owned(),
            0,
            Some("parent_skipped".to_owned())
        )
    );
    assert_eq!(
        outbox_row(&db, "00000000-0000-7000-8000-00000000a033").0,
        "skipped"
    );
    assert_eq!(
        outbox_row(&db, "00000000-0000-7000-8000-00000000a034").0,
        "pending",
        "other items are untouched"
    );
    assert_eq!(prune_sent(&db, 31 * DAY_MS).unwrap(), 3);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM sync_outbox"), 1);

    // The unlink (Task 5) skips every open item in one statement, parents and refunds alike.
    exec(
        &db,
        "INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, idempotency_key, depends_on, created_at)
           VALUES ('00000000-0000-7000-8000-00000000a035', 'cloud', '141', 'pos_refund', 'r4', 'refund:r4',
                   '00000000-0000-7000-8000-00000000a034', 4);
         UPDATE sync_outbox SET status = 'skipped', last_error = 'unlinked'
         WHERE target = 'cloud' AND status IN ('pending', 'failed');",
    )
    .unwrap();
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM sync_outbox WHERE status = 'skipped'"
        ),
        2
    );
    assert_eq!(prune_sent(&db, 31 * DAY_MS + 5).unwrap(), 2);
}

#[test]
/// Migration 0006's guards: the online backup is gone since 1.0.0, its tables and old rows stay.
fn one_active_backup_key_frozen_and_kept() {
    let db = Db::open_memory().unwrap();
    exec(
        &db,
        "INSERT INTO cloud_backup_keys (id, recipient, wrapped_identity, created_at)
         VALUES ('00000000-0000-7000-8000-00000000b001', 'age1first', X'00', 0)",
    )
    .unwrap();
    let second = "INSERT INTO cloud_backup_keys (id, recipient, wrapped_identity, created_at)
         VALUES ('00000000-0000-7000-8000-00000000b002', 'age1second', X'00', 1)";
    assert!(exec(&db, second).is_err(), "one active key");
    assert!(exec(&db, "UPDATE cloud_backup_keys SET wrapped_identity = X'01'").is_err());
    exec(&db, "UPDATE cloud_backup_keys SET retired_at = 2").unwrap();
    exec(&db, second).unwrap();
    assert!(exec(&db, "DELETE FROM cloud_backup_keys").is_err());
    assert!(
        exec(
            &db,
            "INSERT INTO cloud_backup_keys (id, recipient, wrapped_identity, created_at)
             VALUES ('00000000-0000-7000-8000-00000000b003', 'not-a-recipient', X'00', 3)"
        )
        .is_err()
    );
}

#[test]
fn online_orders_are_kept_and_their_states_checked() {
    let db = Db::open_memory().unwrap();
    exec(
        &db,
        "INSERT INTO online_orders (id, store_id, remote_id, order_number, remote_status, payment_method,
                                    delivery_type, subtotal, shipping_cost, discount, total, local_state,
                                    remote_created_at, remote_updated_at, created_at, updated_at)
         VALUES ('00000000-0000-7000-8000-00000000d001', '141', '9001', '1042', 'confirmed', 'cod', 'pickup',
                 450000, 0, 0, 450000, 'new', 1, 1, 1, 1);",
    )
    .unwrap();
    assert!(exec(&db, "UPDATE online_orders SET local_state = 'lost'").is_err());
    assert!(exec(&db, "DELETE FROM online_orders").is_err());
    assert!(
        exec(
            &db,
            "INSERT INTO online_orders (id, store_id, remote_id, order_number, remote_status, payment_method,
                                        delivery_type, subtotal, shipping_cost, discount, total, local_state,
                                        remote_created_at, remote_updated_at, created_at, updated_at)
             VALUES ('00000000-0000-7000-8000-00000000d002', '141', '9001', '1042', 'confirmed', 'cod', 'pickup',
                     1, 0, 0, 1, 'new', 1, 1, 1, 1);"
        )
        .is_err(),
        "one mirror per remote order"
    );
}

fn m1_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/m1-first-sale.db")
}

#[test]
fn the_m1_fixture_upgrades_with_a_silent_feed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    std::fs::copy(m1_fixture_path(), &path).unwrap();
    let db = Db::open(
        &path,
        DbOptions {
            readers: 1,
            backup_dir: Some(dir.path().join("backups")),
        },
    )
    .unwrap();
    assert_eq!(db.report().schema_version, migrations::LATEST_VERSION);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM changelog"), 0);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM cloud_links"), 0);
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
