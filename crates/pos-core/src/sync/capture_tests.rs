//! `ensure` puts back the LAN triggers the schema lost (M5 Task 4 review). SQLite's DROP TABLE
//! drops the table's triggers, so a later migration that rebuilds a replicated table the 12-step
//! way with the same columns (to change a CHECK) keeps the signature and loses the capture and
//! guard triggers: a till's tickets would never reach its outbox. `ensure` compares the triggers
//! in the schema with the generated ones as well. The module's own tests are in `capture.rs`.

use super::capture::*;
use crate::db::{Db, meta_get};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::PaymentInput;
use crate::sales::{Cart, CartLine, SaleInput, create_sale};
use crate::testkit::{Shop, shop};

const CONSUMER: &str = "INSERT INTO sync_cursors (name, kind, last_seq, updated_at)
    VALUES ('changelog:lan:test', 'changelog', 0, 0)";

/// `product_barcodes` rebuilt the 12-step way: the same columns, one CHECK more.
const REBUILD_BARCODES: &str = "
CREATE TABLE product_barcodes_new (
  id         TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  product_id TEXT NOT NULL REFERENCES products (id),
  barcode    TEXT NOT NULL UNIQUE CHECK (length(barcode) BETWEEN 1 AND 64),
  pack_qty   INTEGER NOT NULL DEFAULT 1000 CHECK (pack_qty > 0),
  label      TEXT,
  created_at INTEGER NOT NULL
) STRICT;
INSERT INTO product_barcodes_new SELECT * FROM product_barcodes;
DROP TABLE product_barcodes;
ALTER TABLE product_barcodes_new RENAME TO product_barcodes;
CREATE INDEX idx_product_barcodes_product_id ON product_barcodes (product_id);";

/// The LAN triggers in the schema, `(name, sql)` sorted by name.
fn in_schema(db: &Db) -> Vec<(String, String)> {
    db.read(|c| {
        let mut stmt = c.prepare(
            "SELECT name, sql FROM sqlite_master WHERE type = 'trigger'
               AND (name LIKE 'lan\\_cap\\_%' ESCAPE '\\' OR name LIKE 'lan\\_guard\\_%' ESCAPE '\\')
             ORDER BY name",
        )?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .unwrap()
}

/// The triggers `set` generates for this schema, sorted by name.
fn generated(db: &Db, set: &CaptureSet) -> Vec<(String, String)> {
    let set = set.clone();
    let mut all = db.read(move |c| statements(c, &set)).unwrap();
    all.sort();
    all
}

fn run_ensure(db: &Db, set: &CaptureSet) -> bool {
    let set = set.clone();
    db.write(move |tx| ensure(tx, &set)).unwrap()
}

/// The shop's terminal becomes a till with the LAN's feed reader registered.
fn as_till(shop: &Shop) -> CaptureSet {
    let set = CaptureSet::Client {
        terminal_id: shop.terminal.id.clone(),
    };
    let installing = set.clone();
    shop.db
        .write(move |tx| {
            install(tx, &installing)?;
            tx.execute_batch(CONSUMER)?;
            Ok(())
        })
        .unwrap();
    set
}

#[test]
fn a_table_rebuilt_with_the_same_columns_gets_its_guards_back() {
    let shop = shop();
    let milk = shop.product("Lait", Money(10_000), &shop.tva19);
    let set = as_till(&shop);
    assert_eq!(
        in_schema(&shop.db),
        generated(&shop.db, &set),
        "install puts the generated statements in the schema as they are"
    );

    shop.db
        .write(|tx| Ok(tx.execute_batch(REBUILD_BARCODES)?))
        .unwrap();
    let signing = set.clone();
    let (wanted, recorded) = shop
        .db
        .read(move |c| Ok((signature(c, &signing)?, meta_get(c, "lan_triggers_sig")?)))
        .unwrap();
    assert_eq!(Some(wanted), recorded, "same columns, same signature");
    let left: Vec<String> = in_schema(&shop.db).into_iter().map(|t| t.0).collect();
    assert!(
        !left
            .iter()
            .any(|n| n.starts_with("lan_guard_product_barcodes_")),
        "DROP TABLE took the table's triggers: {left:?}"
    );

    assert!(
        run_ensure(&shop.db, &set),
        "the missing triggers are reinstalled"
    );
    assert_eq!(in_schema(&shop.db), generated(&shop.db, &set));
    let pid = milk.id.clone();
    let err = shop
        .db
        .write(move |tx| {
            tx.execute(
                "INSERT INTO product_barcodes (id, product_id, barcode, created_at)
                 VALUES ('00000000-0000-7000-8000-00000000b0c2', ?1, '6130000000024', 0)",
                [&pid],
            )?;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(err.code(), "host_required", "the till's guard is back");
    assert!(!run_ensure(&shop.db, &set), "then nothing to do");
}

#[test]
fn a_dropped_capture_trigger_is_put_back_and_the_till_s_tickets_reach_the_feed() {
    let shop = shop();
    let milk = shop.product("Lait", Money(10_000), &shop.tva19);
    shop.stock_in(&milk.id, 10_000);
    shop.open_till(0);
    let set = as_till(&shop);
    shop.db
        .write(|tx| Ok(tx.execute_batch("DROP TRIGGER lan_cap_documents_insert")?))
        .unwrap();

    assert!(
        run_ensure(&shop.db, &set),
        "a missing capture trigger is reinstalled"
    );
    let names: Vec<String> = in_schema(&shop.db).into_iter().map(|t| t.0).collect();
    assert!(names.contains(&"lan_cap_documents_insert".to_owned()));
    assert_eq!(in_schema(&shop.db), generated(&shop.db, &set));

    let ctx = shop.till();
    let input = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(&milk.id, Qty(1_000))]),
        payments: vec![PaymentInput::cash(Money(10_500))],
    };
    shop.db
        .write(move |tx| create_sale(tx, &ctx, input))
        .unwrap();
    assert_eq!(
        shop.count(
            "SELECT COUNT(*) FROM changelog WHERE table_name = 'documents' AND op = 'insert'
               AND json_extract(payload, '$._lan') = 1"
        ),
        1,
        "the ticket is in the till's feed"
    );
}

#[test]
fn a_changed_or_stray_lan_trigger_is_replaced() {
    let db = Db::open_memory().unwrap();
    db.write(|tx| install(tx, &CaptureSet::Host)).unwrap();
    db.write(|tx| {
        Ok(tx.execute_batch(
            "DROP TRIGGER lan_cap_products_update;
             CREATE TRIGGER lan_cap_products_update AFTER UPDATE ON products BEGIN SELECT 1; END;
             CREATE TRIGGER lan_guard_units_insert BEFORE INSERT ON units
             BEGIN SELECT RAISE(ABORT, 'host_required'); END;",
        )?)
    })
    .unwrap();

    assert!(
        run_ensure(&db, &CaptureSet::Host),
        "a changed body or a trigger of another set is a mismatch"
    );
    let now = in_schema(&db);
    assert_eq!(now, generated(&db, &CaptureSet::Host));
    assert!(
        !now.iter().any(|t| t.0 == "lan_guard_units_insert"),
        "the main PC carries no guard"
    );
    assert!(!run_ensure(&db, &CaptureSet::Host));
}
