//! Migration 0008 (vertical packs): tables, seeds, append-only ledgers, balance guards, frozen
//! states, the Gérant's new permission, and the upgrade of a pre-M8 database.

use super::*;
use crate::audit;
use crate::auth::Permission;
use crate::auth::roles::{self, MANAGER_ROLE_ID};
use crate::company::{PriceMode, TaxRegime};
use crate::documents;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sales::{Cart, CartLine, SaleInput, create_sale};
use rusqlite::Connection;
use std::path::PathBuf;

/// The schema the pre-M8 fixture was written at: 0001–0005 on the M8 branch (M7's 0006 and
/// M5's 0007 were not on it yet). On main 0006 and 0007 come before 0008, so the fixture
/// upgrades through all three.
const PRE_M8_SCHEMA: i64 = 5;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pre-m8-shop.db")
}

/// Writes the pre-M8 fixture: the database of the release before M8 (schema 5 on that branch) with
/// a shop, a customer who bought on credit, a product in stock and a validated ticket. Run once,
/// in Task 1 Step 1, before 0008 is listed and before `Permission::PharmacyRegister` exists.
#[test]
#[ignore = "writes tests/fixtures/pre-m8-shop.db once, before 0008 is listed"]
fn write_pre_m8_fixture() {
    let path = fixture_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);
    let db = Db::open(&path, DbOptions::default()).unwrap();
    assert_eq!(
        db.report().schema_version,
        PRE_M8_SCHEMA,
        "write the pre-M8 fixture before listing 0008"
    );
    let shop = crate::testkit::shop_in(db, TaxRegime::Reel, PriceMode::Ttc);
    let doliprane = shop.product("Doliprane 500", Money(25_000), &shop.tva9);
    shop.stock_in(&doliprane.id, 12_000);
    let karim = shop.customer("Karim Bensalah");
    shop.open_till(1_000_000);
    let ctx = shop.owner_till();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&doliprane.id, Qty(2_000))]);
    cart.customer_id = Some(karim.id.clone());
    let input = SaleInput {
        cart,
        payments: vec![PaymentInput {
            method: PaymentMethod::Credit,
            amount: Money(50_000),
            reference: None,
            cheque_bank: None,
            cheque_date: None,
        }],
    };
    shop.db
        .write(move |tx| create_sale(tx, &ctx, input))
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

/// A role, a user, a customer and a product to hang pack rows on (ids end in `…f0xx`).
const BASE: &str = "
    INSERT INTO roles (id, code, name_ar, name_fr, name_en, is_system, permissions, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-000000000001', 'owner', 'المالك', 'Propriétaire', 'Owner', 1, '[]', 0, 0);
    INSERT INTO users (id, username, display_name, pin_hash, role_id, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000f001', 'amine', 'Amine', 'x',
              '00000000-0000-7000-8000-000000000001', 0, 0);
    INSERT INTO customers (id, code, name, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000f002', 'C00001', 'Karim Bensalah', 0, 0);
    INSERT INTO tax_rates (id, code, rate_bp, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000f003', 'TVA19', 1900, 0, 0);
    INSERT INTO products (id, name_fr, unit_id, tax_rate_id, sell_price, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000f004', 'Bague or 18 carats',
              '00000000-0000-7000-8000-000000000101', '00000000-0000-7000-8000-00000000f003', 4500000, 0, 0);";

#[test]
fn the_pack_tables_units_and_seeds_exist() {
    let db = Db::open_memory().unwrap();
    assert_eq!(db.report().schema_version, 9);
    for name in [
        "open_price_products",
        "department_keys",
        "product_regulations",
        "dining_rooms",
        "dining_tables",
        "dining_tickets",
        "dining_lines",
        "modifier_groups",
        "modifier_options",
        "product_modifier_groups",
        "kitchen_stations",
        "category_stations",
        "kitchen_tickets",
        "daily_menu",
        "pharmacy_products",
        "chifa_prices",
        "controlled_register",
        "yield_templates",
        "yield_template_cuts",
        "yield_runs",
        "yield_run_cuts",
        "flexy_float",
        "deposits",
        "service_tickets",
        "service_ticket_events",
        "service_ticket_items",
        "instalment_plans",
        "instalment_dues",
        "instalment_guarantees",
        "instalment_events",
        "customer_prices",
        "product_wholesale",
        "delivery_rounds",
        "delivery_round_stops",
        "customer_orders",
        "order_events",
        "stock_reservations",
        "appointments",
        "loyalty_cards",
        "gift_vouchers",
        "gift_voucher_ledger",
        "loyalty_ledger",
        "jewellery_pieces",
        "metal_prices",
        "jeweller_register",
        "staff_profiles",
        "staff_ledger",
        "zakat_statements",
        "payment_types",
        "payment_type_uses",
        "customer_consents",
    ] {
        assert!(table_exists(&db, name), "{name}");
    }
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM units WHERE code IN ('sachet', 'm3', 'bag', 'pallet', 't', 'lot')"
        ),
        6
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM payment_types WHERE code = 'ccp_debit' AND base_method = 'transfer'"
        ),
        1
    );
}

/// A medicine's `pharmacy_products.prescription_required` flag describes the product (it is
/// sold on prescription), not a person: it is the one column the name filter lets through.
#[test]
fn health_data_lives_in_the_controlled_register_only() {
    let db = Db::open_memory().unwrap();
    let health = db
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT m.name FROM sqlite_master m JOIN pragma_table_info(m.name) p
                 WHERE m.type = 'table' AND (p.name LIKE '%patient%' OR p.name LIKE '%prescri%'
                       OR p.name LIKE '%diagnos%' OR p.name LIKE '%religio%')
                   AND NOT (m.name = 'pharmacy_products' AND p.name = 'prescription_required')
                 GROUP BY m.name",
            )?;
            let names = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(names)
        })
        .unwrap();
    assert_eq!(
        health,
        ["controlled_register"],
        "PDP-009: no health field elsewhere"
    );
}

#[test]
fn ledgers_are_append_only() {
    let db = Db::open_memory().unwrap();
    exec(&db, BASE).unwrap();
    exec(
        &db,
        "INSERT INTO deposits (id, owner_type, owner_id, customer_id, kind, amount, method, receipt_number,
                               business_date, created_at, user_id, terminal_id)
           VALUES ('00000000-0000-7000-8000-00000000f101', 'service_ticket', '00000000-0000-7000-8000-00000000f199',
                   '00000000-0000-7000-8000-00000000f002', 'deposit', 200000, 'cash', 'AC2026-000001',
                   '2026-09-28', 1, 'u', 't');
         INSERT INTO flexy_float (id, operator, kind, amount, cost, business_date, created_at, user_id, terminal_id)
           VALUES ('00000000-0000-7000-8000-00000000f102', 'mobilis', 'purchase', 1000000, 960000, '2026-09-28', 1, 'u', 't');
         INSERT INTO metal_prices (id, metal, titre_permille, price_per_gram, effective_at, created_at, user_id)
           VALUES ('00000000-0000-7000-8000-00000000f103', 'gold', 750, 1450000, 1, 1, 'u');
         INSERT INTO customer_consents (id, customer_id, channel, granted, source, created_at, user_id)
           VALUES ('00000000-0000-7000-8000-00000000f104', '00000000-0000-7000-8000-00000000f002', 'sms', 1, 'counter', 1, 'u');",
    )
    .unwrap();
    for table in [
        "deposits",
        "flexy_float",
        "metal_prices",
        "customer_consents",
    ] {
        let update = format!("UPDATE {table} SET created_at = 2");
        let delete = format!("DELETE FROM {table}");
        let upd = db.write(move |tx| Ok(tx.execute_batch(&update)?));
        let del = db.write(move |tx| Ok(tx.execute_batch(&delete)?));
        assert!(upd.is_err() && del.is_err(), "{table} is append-only");
    }
}

#[test]
fn balances_never_go_below_zero() {
    let db = Db::open_memory().unwrap();
    exec(&db, BASE).unwrap();
    exec(
        &db,
        "INSERT INTO deposits (id, owner_type, owner_id, customer_id, kind, amount, method, receipt_number,
                               business_date, created_at, user_id, terminal_id)
           VALUES ('00000000-0000-7000-8000-00000000f201', 'customer_order', '00000000-0000-7000-8000-00000000f299',
                   '00000000-0000-7000-8000-00000000f002', 'deposit', 100000, 'cash', 'AC2026-000002',
                   '2026-09-28', 1, 'u', 't');",
    )
    .unwrap();
    let refund = exec(
        &db,
        "INSERT INTO deposits (id, owner_type, owner_id, kind, amount, method, business_date, created_at, user_id, terminal_id)
           VALUES ('00000000-0000-7000-8000-00000000f202', 'customer_order', '00000000-0000-7000-8000-00000000f299',
                   'refund', -100001, 'cash', '2026-09-28', 2, 'u', 't');",
    );
    assert!(refund.is_err(), "more than the deposit held");
    let sale = exec(
        &db,
        "INSERT INTO flexy_float (id, operator, kind, amount, business_date, created_at, user_id, terminal_id)
           VALUES ('00000000-0000-7000-8000-00000000f203', 'djezzy', 'sale', -10000, '2026-09-28', 1, 'u', 't');",
    );
    assert!(sale.is_err(), "a top-up needs float");
    let points = exec(
        &db,
        "INSERT INTO loyalty_ledger (id, customer_id, kind, points, business_date, created_at, user_id, terminal_id)
           VALUES ('00000000-0000-7000-8000-00000000f204', '00000000-0000-7000-8000-00000000f002', 'redeem', -1,
                   '2026-09-28', 1, 'u', 't');",
    );
    assert!(points.is_err(), "no points to redeem");
    // A redemption also needs its voucher: an expiry isolates the balance guard itself.
    let expiry = exec(
        &db,
        "INSERT INTO loyalty_ledger (id, customer_id, kind, points, business_date, created_at, user_id, terminal_id)
           VALUES ('00000000-0000-7000-8000-00000000f206', '00000000-0000-7000-8000-00000000f002', 'expire', -1,
                   '2026-09-28', 1, 'u', 't');",
    );
    assert!(expiry.is_err(), "no points to expire");
    let advance = exec(
        &db,
        "INSERT INTO staff_ledger (id, user_id, kind, amount, business_date, created_at, created_by, terminal_id)
           VALUES ('00000000-0000-7000-8000-00000000f205', '00000000-0000-7000-8000-00000000f001', 'repayment', -5000,
                   '2026-09-28', 1, 'u', 't');",
    );
    assert!(advance.is_err(), "nothing owed");
}

#[test]
fn the_schema_refuses_a_cheque_as_a_guarantee() {
    let db = Db::open_memory().unwrap();
    let sql = "INSERT INTO instalment_guarantees (id, plan_id, kind, created_at, user_id)
               VALUES ('00000000-0000-7000-8000-00000000f301', '00000000-0000-7000-8000-00000000f399', 'cheque', 1, 'u')";
    let err = exec(&db, sql).unwrap_err();
    assert_eq!(
        err.code(),
        "constraint_violation",
        "TMP-009 at the lowest level"
    );
    // The plan does not exist either: with its foreign key deferred (and the write rolled back),
    // only the kind decides, so a neutral document goes in and a cheque does not.
    let guarantee = |kind: &'static str| {
        db.dry_run(move |tx| {
            tx.execute_batch("PRAGMA defer_foreign_keys = ON")?;
            tx.execute(
                "INSERT INTO instalment_guarantees (id, plan_id, kind, created_at, user_id)
                 VALUES ('00000000-0000-7000-8000-00000000f302',
                         '00000000-0000-7000-8000-00000000f399', ?1, 1, 'u')",
                [kind],
            )?;
            Ok(())
        })
    };
    guarantee("id_document").unwrap();
    assert_eq!(
        guarantee("cheque").unwrap_err().code(),
        "constraint_violation"
    );
}

#[test]
fn final_states_are_frozen() {
    let db = Db::open_memory().unwrap();
    exec(&db, BASE).unwrap();
    exec(
        &db,
        "INSERT INTO dining_tickets (id, mode, order_no, business_date, covers, waiter_id, lang, status,
                                     opened_at, closed_at, document_id, terminal_id, user_id, created_at, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000f401', 'takeaway', 1, '2026-09-28', 0,
                   '00000000-0000-7000-8000-00000000f001', 'fr', 'void', 1, 2, NULL, 't', 'u', 1, 1);",
    )
    .unwrap();
    assert!(
        exec(
            &db,
            "UPDATE dining_tickets SET status = 'open', closed_at = NULL"
        )
        .is_err()
    );
    let late_line = exec(
        &db,
        "INSERT INTO dining_lines (id, ticket_id, position, cart_line, qty, user_id, created_at, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000f402', '00000000-0000-7000-8000-00000000f401', 1,
                   '{\"qty\":1000}', 1000, 'u', 1, 1);",
    );
    assert!(late_line.is_err(), "a void ticket takes no line");
    exec(
        &db,
        "INSERT INTO stock_reservations (id, product_id, location_id, qty, owner_type, owner_id, created_at, user_id)
           VALUES ('00000000-0000-7000-8000-00000000f403', '00000000-0000-7000-8000-00000000f004',
                   '00000000-0000-7000-8000-000000000301', 1000, 'customer_order',
                   '00000000-0000-7000-8000-00000000f499', 1, 'u');
         UPDATE stock_reservations SET released_at = 5;",
    )
    .unwrap();
    assert!(
        exec(&db, "UPDATE stock_reservations SET released_at = 6").is_err(),
        "released once"
    );
    assert!(exec(&db, "UPDATE stock_reservations SET qty = 2000").is_err());
    // A round out with two stops: the delivered one is linked to its versement while the round is
    // out (as `rounds::close` does), then the round closes and nothing of its stops moves again,
    // the link included.
    exec(
        &db,
        "INSERT INTO documents (id, doc_type, business_date, customer_id, user_id, terminal_id, created_at, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000f404', 'delivery_note', '2026-09-28',
                   '00000000-0000-7000-8000-00000000f002', 'u', 't', 1, 1);
         INSERT INTO customer_ledger (id, customer_id, kind, amount, balance_after, business_date, created_at)
           VALUES ('00000000-0000-7000-8000-00000000f405', '00000000-0000-7000-8000-00000000f002', 'opening',
                   5000, 5000, '2026-09-28', 1);
         INSERT INTO delivery_rounds (id, number, business_date, driver_name, status, started_at,
                                      created_at, updated_at, user_id, terminal_id)
           VALUES ('00000000-0000-7000-8000-00000000f406', 'TR2026-000001', '2026-09-28', 'Rachid', 'out', 1,
                   1, 1, 'u', 't');
         INSERT INTO delivery_round_stops (round_id, seq, document_id, customer_id, amount_due, status,
                                           collected, method, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000f406', 1, '00000000-0000-7000-8000-00000000f404',
                   '00000000-0000-7000-8000-00000000f002', 5000, 'delivered', 5000, 'cash', 1),
                  ('00000000-0000-7000-8000-00000000f406', 2, '00000000-0000-7000-8000-00000000f404',
                   '00000000-0000-7000-8000-00000000f002', 5000, 'failed', 0, NULL, 1);
         UPDATE delivery_round_stops SET ledger_entry_id = '00000000-0000-7000-8000-00000000f405', updated_at = 2
           WHERE seq = 1;
         UPDATE delivery_rounds SET status = 'closed', closed_at = 2, updated_at = 2;",
    )
    .unwrap();
    for frozen in [
        "UPDATE delivery_round_stops SET collected = 0, method = NULL WHERE seq = 1",
        // Moving the link along with the rest opened the freeze of the brief's DDL.
        "UPDATE delivery_round_stops SET collected = 1, amount_due = 0, status = 'failed',
                ledger_entry_id = NULL WHERE seq = 1",
        "UPDATE delivery_round_stops SET ledger_entry_id = NULL WHERE seq = 1",
        "UPDATE delivery_round_stops SET ledger_entry_id = '00000000-0000-7000-8000-00000000f405'
           WHERE seq = 2",
        "INSERT INTO delivery_round_stops (round_id, seq, document_id, customer_id, amount_due, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000f406', 3, '00000000-0000-7000-8000-00000000f404',
                   '00000000-0000-7000-8000-00000000f002', 5000, 3)",
        "DELETE FROM delivery_round_stops WHERE seq = 2",
        "UPDATE delivery_rounds SET status = 'out', closed_at = NULL",
    ] {
        assert!(exec(&db, frozen).is_err(), "a closed round is final: {frozen}");
    }
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM delivery_round_stops
             WHERE (seq = 1 AND collected = 5000 AND amount_due = 5000 AND status = 'delivered'
                    AND ledger_entry_id = '00000000-0000-7000-8000-00000000f405')
                OR (seq = 2 AND collected = 0 AND ledger_entry_id IS NULL)"
        ),
        2
    );
}

/// `printed_at`, `released_at` and `blocked_at` each move once and alone: any other column moved
/// in the same UPDATE, the row's owner, author, date or id included, is refused.
#[test]
fn printed_released_and_blocked_move_alone() {
    let db = Db::open_memory().unwrap();
    exec(&db, BASE).unwrap();
    exec(
        &db,
        "INSERT INTO dining_tickets (id, mode, order_no, business_date, covers, waiter_id, lang, status,
                                     opened_at, terminal_id, user_id, created_at, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000f501', 'takeaway', 1, '2026-09-28', 0,
                   '00000000-0000-7000-8000-00000000f001', 'fr', 'open', 1, 't', 'u', 1, 1);
         INSERT INTO kitchen_tickets (id, ticket_id, seq, kind, lines, created_at, user_id, terminal_id)
           VALUES ('00000000-0000-7000-8000-00000000f502', '00000000-0000-7000-8000-00000000f501', 1,
                   'order', '[{\"qty\":1000}]', 1, 'u', 't');
         INSERT INTO stock_reservations (id, product_id, location_id, qty, owner_type, owner_id, created_at, user_id)
           VALUES ('00000000-0000-7000-8000-00000000f503', '00000000-0000-7000-8000-00000000f004',
                   '00000000-0000-7000-8000-000000000301', 1000, 'customer_order',
                   '00000000-0000-7000-8000-00000000f599', 1, 'u');
         INSERT INTO loyalty_cards (code, customer_id, issued_at)
           VALUES ('LC00000001', '00000000-0000-7000-8000-00000000f002', 1);",
    )
    .unwrap();
    for moved in [
        "UPDATE kitchen_tickets SET printed_at = 5, terminal_id = 'other'",
        "UPDATE kitchen_tickets SET printed_at = 5, user_id = 'z'",
        "UPDATE kitchen_tickets SET printed_at = 5, created_at = 0",
        "UPDATE kitchen_tickets SET printed_at = 5, id = '00000000-0000-7000-8000-00000000f5ff'",
        "UPDATE stock_reservations SET released_at = 5, user_id = 'someone'",
        "UPDATE stock_reservations SET released_at = 5, created_at = 99",
        "UPDATE stock_reservations SET released_at = 5, id = '00000000-0000-7000-8000-00000000f5fe'",
        "UPDATE loyalty_cards SET issued_at = 0",
        "UPDATE loyalty_cards SET blocked_at = 5, issued_at = 0",
    ] {
        assert!(exec(&db, moved).is_err(), "{moved}");
    }
    exec(
        &db,
        "UPDATE kitchen_tickets SET printed_at = 5;
         UPDATE stock_reservations SET released_at = 5;
         UPDATE loyalty_cards SET blocked_at = 5;",
    )
    .unwrap();
    assert_eq!(
        count(
            &db,
            "SELECT (SELECT COUNT(*) FROM kitchen_tickets
                     WHERE id = '00000000-0000-7000-8000-00000000f502' AND printed_at = 5
                       AND terminal_id = 't' AND user_id = 'u' AND created_at = 1)
                  + (SELECT COUNT(*) FROM stock_reservations
                     WHERE id = '00000000-0000-7000-8000-00000000f503' AND released_at = 5
                       AND user_id = 'u' AND created_at = 1)
                  + (SELECT COUNT(*) FROM loyalty_cards WHERE blocked_at = 5 AND issued_at = 1)"
        ),
        3
    );
}

#[test]
fn a_fresh_manager_role_holds_the_register_permission() {
    let db = Db::open_memory().unwrap();
    db.write(|tx| roles::seed_system_roles(tx)).unwrap();
    let granted = db
        .read(|c| roles::permissions_of(c, MANAGER_ROLE_ID))
        .unwrap();
    assert!(granted.contains(&Permission::PharmacyRegister));
}

#[test]
fn the_pre_m8_fixture_upgrades_in_place_after_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    std::fs::copy(fixture_path(), &path).unwrap();
    let db = Db::open(&path, DbOptions::default()).unwrap();
    let report = db.report().clone();
    assert_eq!((report.schema_version, report.migrations_applied), (9, 4));
    let backup = report
        .pre_migration_backup
        .expect("a backup before migrating");
    let copy = Connection::open(&backup).unwrap();
    assert_eq!(schema_version(&copy).unwrap(), PRE_M8_SCHEMA);
    assert!(integrity_ok(&copy).unwrap());
    drop(copy);
    assert!(db.integrity_check().unwrap());
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM pragma_foreign_key_check"),
        0
    );
    assert_eq!(db.read(audit::verify_chain).unwrap().first_broken_seq, None);
    assert!(
        db.read(documents::verify_all)
            .unwrap()
            .iter()
            .all(|r| r.first_broken_seq.is_none())
    );
    assert_eq!(count(&db, "SELECT balance FROM customers"), 50_000);
    let granted = db
        .read(|c| roles::permissions_of(c, MANAGER_ROLE_ID))
        .unwrap();
    assert!(
        granted.contains(&Permission::PharmacyRegister),
        "the Gérant was upgraded"
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM json_each((SELECT permissions FROM roles WHERE id = '00000000-0000-7000-8000-000000000003')) WHERE value = 'pharmacy.register'"
        ),
        0,
        "the Caissier was not"
    );
    exec(
        &db,
        "INSERT INTO pharmacy_products (product_id, dci, dci_key, controlled, updated_at)
           SELECT id, 'Paracétamol', 'paracetamol', 0, 1 FROM products LIMIT 1;",
    )
    .unwrap();
}
