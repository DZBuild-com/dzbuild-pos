//! Migration 0003 (back office): tables, seeds, append-only ledgers and their balance caches,
//! lot levels, frozen counts, generated soldes flag, and the upgrade of an M1 database.

use super::*;
use crate::audit;
use crate::company::{PriceMode, TaxRegime};
use crate::documents;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::PaymentInput;
use crate::sales::{Cart, CartLine, SaleInput, create_sale};
use rusqlite::Connection;
use std::path::PathBuf;

fn m1_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/m1-first-sale.db")
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

const TAX_AND_PRODUCT: &str = "
    INSERT INTO tax_rates (id, code, rate_bp, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000e401', 'TVA19', 1900, 0, 0);
    INSERT INTO products (id, name_fr, unit_id, tax_rate_id, sell_price, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000e001', 'Doliprane 500', '00000000-0000-7000-8000-000000000101',
              '00000000-0000-7000-8000-00000000e401', 25000, 0, 0);";

const CUSTOMER: &str = "INSERT INTO customers (id, code, name, created_at, updated_at)
    VALUES ('00000000-0000-7000-8000-00000000d001', 'C00001', 'Karim Bensalah', 0, 0);";

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

#[test]
fn the_back_office_tables_and_seeds_exist() {
    let db = Db::open_memory().unwrap();
    assert_eq!(db.report().schema_version, 10);
    for name in [
        "customers",
        "customer_ledger",
        "customer_allocations",
        "suppliers",
        "supplier_ledger",
        "supplier_allocations",
        "expense_categories",
        "expenses",
        "stock_counts",
        "stock_count_lines",
        "lots",
        "lot_levels",
        "serials",
        "promotions",
        "promotion_products",
        "sale_line_promotions",
        "price_breaks",
        "product_extras",
        "location_minimums",
        "stock_transfers",
        "cost_history",
        "import_jobs",
        "import_job_rows",
    ] {
        assert!(table_exists(&db, name), "{name}");
    }
    assert_eq!(count(&db, "SELECT COUNT(*) FROM expense_categories"), 8);
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM expense_categories WHERE code = cash_group"
        ),
        8
    );
}

#[test]
fn the_customer_ledger_moves_the_cached_balance_and_is_append_only() {
    let db = Db::open_memory().unwrap();
    exec(&db, CUSTOMER).unwrap();
    exec(
        &db,
        "INSERT INTO customer_ledger (id, customer_id, kind, amount, balance_after, business_date, created_at)
         VALUES ('00000000-0000-7000-8000-00000000d101', '00000000-0000-7000-8000-00000000d001',
                 'opening', 150000, 150000, '2026-09-28', 1)",
    )
    .unwrap();
    assert_eq!(count(&db, "SELECT balance FROM customers"), 150_000);
    let off_cache = exec(
        &db,
        "INSERT INTO customer_ledger (id, customer_id, kind, amount, balance_after, method, business_date, created_at)
         VALUES ('00000000-0000-7000-8000-00000000d102', '00000000-0000-7000-8000-00000000d001',
                 'payment', -50000, 999, 'cash', '2026-09-28', 2)",
    );
    assert!(off_cache.is_err(), "balance_after must follow the cache");
    exec(
        &db,
        "INSERT INTO customer_ledger (id, customer_id, kind, amount, balance_after, method, business_date, created_at)
         VALUES ('00000000-0000-7000-8000-00000000d102', '00000000-0000-7000-8000-00000000d001',
                 'payment', -50000, 100000, 'cash', '2026-09-28', 2)",
    )
    .unwrap();
    assert_eq!(count(&db, "SELECT balance FROM customers"), 100_000);
    assert!(exec(&db, "UPDATE customer_ledger SET note = 'x'").is_err());
    assert!(exec(&db, "DELETE FROM customer_ledger").is_err());
    assert!(
        exec(&db, "DELETE FROM customers").is_err(),
        "archived, never deleted"
    );
    let wrong_sign = exec(
        &db,
        "INSERT INTO customer_ledger (id, customer_id, kind, amount, balance_after, business_date, created_at)
         VALUES ('00000000-0000-7000-8000-00000000d103', '00000000-0000-7000-8000-00000000d001',
                 'sale_credit', -100, 99900, '2026-09-28', 3)",
    );
    assert!(wrong_sign.is_err(), "a credit sale increases the debt");
    let no_method = exec(
        &db,
        "INSERT INTO customer_ledger (id, customer_id, kind, amount, balance_after, business_date, created_at)
         VALUES ('00000000-0000-7000-8000-00000000d104', '00000000-0000-7000-8000-00000000d001',
                 'payment', -100, 99900, '2026-09-28', 3)",
    );
    assert!(no_method.is_err(), "a versement says how it was paid");
}

#[test]
fn lotted_movements_move_lot_levels_and_plain_ones_do_not() {
    let db = Db::open_memory().unwrap();
    exec(&db, TAX_AND_PRODUCT).unwrap();
    exec(
        &db,
        "INSERT INTO lots (id, product_id, lot_number, expiry_date, created_at, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000e101', '00000000-0000-7000-8000-00000000e001', 'L2409', '2027-03-31', 0, 0);
         INSERT INTO stock_movements (id, product_id, location_id, qty_delta, reason, created_at, lot_id)
           VALUES ('00000000-0000-7000-8000-00000000e201', '00000000-0000-7000-8000-00000000e001',
                   '00000000-0000-7000-8000-000000000301', 12000, 'purchase', 1, '00000000-0000-7000-8000-00000000e101');
         INSERT INTO stock_movements (id, product_id, location_id, qty_delta, reason, created_at, lot_id)
           VALUES ('00000000-0000-7000-8000-00000000e202', '00000000-0000-7000-8000-00000000e001',
                   '00000000-0000-7000-8000-000000000301', -2000, 'sale', 2, '00000000-0000-7000-8000-00000000e101');
         INSERT INTO stock_movements (id, product_id, location_id, qty_delta, reason, created_at)
           VALUES ('00000000-0000-7000-8000-00000000e203', '00000000-0000-7000-8000-00000000e001',
                   '00000000-0000-7000-8000-000000000301', 5000, 'opening', 3);",
    )
    .unwrap();
    assert_eq!(count(&db, "SELECT qty FROM lot_levels"), 10_000);
    assert_eq!(count(&db, "SELECT qty FROM stock_levels"), 15_000);
    assert!(exec(&db, "DELETE FROM lots").is_err());
}

#[test]
fn count_lines_freeze_once_the_count_is_closed() {
    let db = Db::open_memory().unwrap();
    exec(&db, TAX_AND_PRODUCT).unwrap();
    exec(
        &db,
        "INSERT INTO stock_counts (id, location_id, scope, status, started_at, user_id)
           VALUES ('00000000-0000-7000-8000-00000000e301', '00000000-0000-7000-8000-000000000301', 'full', 'open', 1, 'u1');
         INSERT INTO stock_count_lines (count_id, product_id, expected_qty)
           VALUES ('00000000-0000-7000-8000-00000000e301', '00000000-0000-7000-8000-00000000e001', 5000);
         UPDATE stock_count_lines SET counted_qty = 4000;",
    )
    .unwrap();
    let second_open = exec(
        &db,
        "INSERT INTO stock_counts (id, location_id, scope, status, started_at, user_id)
           VALUES ('00000000-0000-7000-8000-00000000e302', '00000000-0000-7000-8000-000000000301', 'full', 'open', 2, 'u1')",
    );
    assert!(second_open.is_err(), "one open count per location");
    exec(
        &db,
        "UPDATE stock_counts SET status = 'closed', closed_at = 5, closed_by = 'u1'",
    )
    .unwrap();
    assert!(exec(&db, "UPDATE stock_count_lines SET counted_qty = 1").is_err());
    assert!(exec(&db, "UPDATE stock_counts SET note = 'x'").is_err());
    assert!(exec(&db, "DELETE FROM stock_counts").is_err());
}

#[test]
fn promotions_flag_legal_soldes_and_check_their_values() {
    let db = Db::open_memory().unwrap();
    exec(
        &db,
        "INSERT INTO promotions (id, name_fr, kind, campaign, value, starts_on, ends_on, created_at, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000e501', 'Soldes d''hiver', 'percent', 'solde', 3000,
                   '2027-01-10', '2027-02-20', 0, 0)",
    )
    .unwrap();
    assert_eq!(count(&db, "SELECT is_legal_soldes FROM promotions"), 1);
    let empty = exec(
        &db,
        "INSERT INTO promotions (id, name_fr, kind, value, created_at, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000e502', 'Rien', 'percent', 0, 0, 0)",
    );
    assert!(empty.is_err(), "a percent promotion takes something off");
    let backwards = exec(
        &db,
        "INSERT INTO promotions (id, name_fr, kind, value, starts_on, ends_on, created_at, updated_at)
           VALUES ('00000000-0000-7000-8000-00000000e503', 'Bug', 'amount', 500, '2027-02-01', '2027-01-01', 0, 0)",
    );
    assert!(backwards.is_err());
}

#[test]
fn expenses_and_trails_are_append_only() {
    let db = Db::open_memory().unwrap();
    exec(
        &db,
        "INSERT INTO expenses (id, category_id, amount, method, business_date, created_at, user_id)
           VALUES ('00000000-0000-7000-8000-00000000e601', '00000000-0000-7000-8000-000000000502',
                   450000, 'transfer', '2026-09-28', 1, 'u1')",
    )
    .unwrap();
    assert!(exec(&db, "UPDATE expenses SET amount = 1").is_err());
    assert!(exec(&db, "DELETE FROM expenses").is_err());
    let unlinked_reversal = exec(
        &db,
        "INSERT INTO expenses (id, category_id, amount, method, business_date, created_at, user_id)
           VALUES ('00000000-0000-7000-8000-00000000e602', '00000000-0000-7000-8000-000000000502',
                   -450000, 'transfer', '2026-09-28', 2, 'u1')",
    );
    assert!(
        unlinked_reversal.is_err(),
        "a negative expense reverses a named one"
    );
    exec(
        &db,
        "INSERT INTO expenses (id, category_id, amount, method, reversal_of, business_date, created_at, user_id)
           VALUES ('00000000-0000-7000-8000-00000000e602', '00000000-0000-7000-8000-000000000502',
                   -450000, 'transfer', '00000000-0000-7000-8000-00000000e601', '2026-09-28', 2, 'u1')",
    )
    .unwrap();
    exec(&db, TAX_AND_PRODUCT).unwrap();
    exec(
        &db,
        "INSERT INTO cost_history (id, product_id, cost_before, cost_after, source_type, created_at)
           VALUES ('00000000-0000-7000-8000-00000000e701', '00000000-0000-7000-8000-00000000e001', 0, 18000, 'purchase', 1)",
    )
    .unwrap();
    assert!(exec(&db, "DELETE FROM cost_history").is_err());
}

/// Every trigger of migrations 0001-0009, by name: a later migration that rebuilds a table and
/// drops one of them silently fails here. 0004 adds the M3 document guards (parties, links,
/// drafts, the M3 fields) and `counter_skips`, and recreates `counters_start_at_one` and
/// `counters_step_by_one` so they accept a journaled skip; 0005 adds none; 0006 adds the change
/// feed (`changelog_*`: the append-only guard and its capture triggers) and the DZBuild link's
/// guards (`cloud_links_kept`, `cloud_backup_keys_*`, `online_orders_kept`, `sync_outbox_*`);
/// 0007 adds the LAN tables' 11 append-only and kept guards (`lan_*`); 0008 adds the 76 guards
/// of the vertical packs (append-only ledgers, balances, frozen states, no deletes); 0009 adds the
/// 4 guards of the per-lot count lines (`stock_count_lot_lines_*`: open count only, a lot of
/// the line's product). The LAN capture triggers are not in any migration: `sync::capture`
/// installs them on a main PC or a till, never on a single PC.
const TRIGGERS: [&str; 194] = [
    "audit_log_no_delete",
    "audit_log_no_update",
    "cash_closures_closed_session",
    "cash_closures_no_delete",
    "cash_closures_no_update",
    "cash_counts_no_delete",
    "cash_counts_no_update",
    "cash_counts_session_open",
    "cash_movements_no_delete",
    "cash_movements_no_update",
    "cash_movements_session_open",
    "cash_sessions_closed_are_final",
    "cash_sessions_no_delete",
    "cash_sessions_open_on_insert",
    "changelog_append_only",
    "changelog_barcodes_delete",
    "changelog_barcodes_insert",
    "changelog_barcodes_update",
    "changelog_cash_closures_insert",
    "changelog_categories_update",
    "changelog_cloud_publish_insert",
    "changelog_cloud_publish_update",
    "changelog_documents_validated",
    "changelog_products_insert",
    "changelog_products_update",
    "changelog_stock_movements_insert",
    "cloud_backup_keys_frozen",
    "cloud_backup_keys_kept",
    "cloud_links_kept",
    "controlled_register_no_delete",
    "controlled_register_no_update",
    "cost_history_no_delete",
    "cost_history_no_update",
    "counter_skips_no_delete",
    "counter_skips_no_update",
    "counters_no_delete",
    "counters_start_at_one",
    "counters_step_by_one",
    "customer_allocations_no_delete",
    "customer_allocations_no_update",
    "customer_consents_no_delete",
    "customer_consents_no_update",
    "customer_ledger_follows_the_cache",
    "customer_ledger_moves_the_balance",
    "customer_ledger_no_delete",
    "customer_ledger_no_update",
    "customer_orders_final",
    "customer_orders_no_delete",
    "customers_no_delete",
    "delivery_round_stops_frozen",
    "delivery_round_stops_frozen_insert",
    "delivery_round_stops_no_delete",
    "delivery_rounds_final",
    "delivery_rounds_no_delete",
    "department_keys_no_delete",
    "deposits_never_negative",
    "deposits_no_delete",
    "deposits_no_update",
    "dining_lines_final_insert",
    "dining_lines_final_update",
    "dining_lines_no_delete",
    "dining_rooms_no_delete",
    "dining_tables_no_delete",
    "dining_tickets_final",
    "dining_tickets_no_delete",
    "document_drafts_draft_insert",
    "document_drafts_frozen",
    "document_drafts_no_delete",
    "document_lines_draft_delete",
    "document_lines_draft_insert",
    "document_lines_draft_update",
    "document_links_no_delete",
    "document_links_no_update",
    "document_parties_draft_delete",
    "document_parties_draft_insert",
    "document_parties_draft_update",
    "document_taxes_draft_delete",
    "document_taxes_draft_insert",
    "document_taxes_draft_update",
    "documents_final_once_validated",
    "documents_insert_as_draft",
    "documents_kept_once_validated",
    "documents_m3_fields_fit_the_kind",
    "expense_categories_no_delete",
    "expenses_no_delete",
    "expenses_no_update",
    "flexy_float_never_negative",
    "flexy_float_no_delete",
    "flexy_float_no_update",
    "gift_voucher_ledger_never_negative",
    "gift_voucher_ledger_no_delete",
    "gift_voucher_ledger_no_update",
    "gift_vouchers_no_delete",
    "gift_vouchers_no_update",
    "import_job_rows_no_delete",
    "import_job_rows_no_update",
    "import_jobs_no_delete",
    "instalment_dues_no_delete",
    "instalment_dues_no_update",
    "instalment_events_no_delete",
    "instalment_events_no_update",
    "instalment_guarantees_no_delete",
    "instalment_guarantees_no_update",
    "instalment_plans_no_delete",
    "instalment_plans_no_update",
    "jeweller_register_no_delete",
    "jeweller_register_no_update",
    "kitchen_stations_no_delete",
    "kitchen_tickets_no_delete",
    "kitchen_tickets_printed_once",
    "lan_devices_kept",
    "lan_inbox_no_delete",
    "lan_inbox_no_update",
    "lan_journal_no_delete",
    "lan_journal_no_update",
    "lan_local_kept",
    "lan_notices_kept",
    "lan_notices_only_acknowledged",
    "lan_outbox_frozen",
    "lan_outbox_kept",
    "lan_outbox_sent_is_final",
    "lots_no_delete",
    "loyalty_cards_blocked_once",
    "loyalty_cards_no_delete",
    "loyalty_ledger_never_negative",
    "loyalty_ledger_no_delete",
    "loyalty_ledger_no_update",
    "metal_prices_no_delete",
    "metal_prices_no_update",
    "modifier_groups_no_delete",
    "modifier_options_no_delete",
    "online_orders_kept",
    "order_events_no_delete",
    "order_events_no_update",
    "payment_type_uses_no_delete",
    "payment_type_uses_no_update",
    "payment_types_no_delete",
    "payments_draft_delete",
    "payments_draft_insert",
    "payments_draft_update",
    "products_fts_after_delete",
    "products_fts_after_insert",
    "products_fts_after_update",
    "promotion_products_no_delete",
    "promotion_products_no_update",
    "promotions_no_delete",
    "sale_line_promotions_no_delete",
    "sale_line_promotions_no_update",
    "serials_no_delete",
    "service_ticket_events_no_delete",
    "service_ticket_events_no_update",
    "service_ticket_items_frozen_delete",
    "service_ticket_items_frozen_insert",
    "service_ticket_items_no_update",
    "service_tickets_final",
    "service_tickets_no_delete",
    "staff_ledger_never_negative",
    "staff_ledger_no_delete",
    "staff_ledger_no_update",
    "stock_count_lines_open_delete",
    "stock_count_lines_open_insert",
    "stock_count_lines_open_update",
    "stock_count_lot_lines_lot_of_product",
    "stock_count_lot_lines_open_delete",
    "stock_count_lot_lines_open_insert",
    "stock_count_lot_lines_open_update",
    "stock_counts_final",
    "stock_counts_no_delete",
    "stock_movements_no_delete",
    "stock_movements_no_update",
    "stock_movements_update_level",
    "stock_movements_update_lot_level",
    "stock_reservations_no_delete",
    "stock_reservations_released_once",
    "stock_transfers_no_delete",
    "stock_transfers_no_update",
    "supplier_allocations_no_delete",
    "supplier_allocations_no_update",
    "supplier_ledger_follows_the_cache",
    "supplier_ledger_moves_the_balance",
    "supplier_ledger_no_delete",
    "supplier_ledger_no_update",
    "suppliers_no_delete",
    "sync_outbox_key_frozen",
    "sync_outbox_open_kept",
    "sync_outbox_payload_frozen",
    "sync_outbox_skip_dependants",
    "yield_run_cuts_no_delete",
    "yield_run_cuts_no_update",
    "yield_runs_no_delete",
    "yield_runs_no_update",
    "yield_templates_no_delete",
    "zakat_statements_no_delete",
    "zakat_statements_no_update",
];

/// One row in every append-only or never-deleted table of 0003 (foreign keys deferred: the
/// transaction is rolled back), the count closed so its lines are final.
const TRAIL_ROWS: &str = "
    PRAGMA defer_foreign_keys = ON;
    INSERT INTO customers (id, code, name, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000d001', 'C00001', 'Karim Bensalah', 0, 0);
    INSERT INTO customer_ledger (id, customer_id, kind, amount, balance_after, business_date, created_at)
      VALUES ('00000000-0000-7000-8000-00000000d101', '00000000-0000-7000-8000-00000000d001',
              'opening', 150000, 150000, '2026-09-28', 1);
    INSERT INTO customer_ledger (id, customer_id, kind, amount, balance_after, method, business_date, created_at)
      VALUES ('00000000-0000-7000-8000-00000000d102', '00000000-0000-7000-8000-00000000d001',
              'payment', -50000, 100000, 'cash', '2026-09-28', 2);
    INSERT INTO customer_allocations (id, customer_id, debit_id, credit_id, amount, created_at)
      VALUES ('00000000-0000-7000-8000-00000000d201', '00000000-0000-7000-8000-00000000d001',
              '00000000-0000-7000-8000-00000000d101', '00000000-0000-7000-8000-00000000d102', 50000, 2);
    INSERT INTO suppliers (id, code, name, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000f001', 'F00001', 'Cevital', 0, 0);
    INSERT INTO supplier_ledger (id, supplier_id, kind, amount, balance_after, business_date, created_at)
      VALUES ('00000000-0000-7000-8000-00000000f101', '00000000-0000-7000-8000-00000000f001',
              'purchase', 300000, 300000, '2026-09-28', 1);
    INSERT INTO supplier_ledger (id, supplier_id, kind, amount, balance_after, method, business_date, created_at)
      VALUES ('00000000-0000-7000-8000-00000000f102', '00000000-0000-7000-8000-00000000f001',
              'payment', -100000, 200000, 'cash', '2026-09-28', 2);
    INSERT INTO supplier_allocations (id, supplier_id, debit_id, credit_id, amount, created_at)
      VALUES ('00000000-0000-7000-8000-00000000f201', '00000000-0000-7000-8000-00000000f001',
              '00000000-0000-7000-8000-00000000f101', '00000000-0000-7000-8000-00000000f102', 100000, 2);
    INSERT INTO expenses (id, category_id, amount, method, business_date, created_at, user_id)
      VALUES ('00000000-0000-7000-8000-00000000e601', '00000000-0000-7000-8000-000000000502',
              450000, 'transfer', '2026-09-28', 1, 'u1');
    INSERT INTO tax_rates (id, code, rate_bp, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000e401', 'TVA19', 1900, 0, 0);
    INSERT INTO products (id, name_fr, unit_id, tax_rate_id, sell_price, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000e001', 'Doliprane 500', '00000000-0000-7000-8000-000000000101',
              '00000000-0000-7000-8000-00000000e401', 25000, 0, 0);
    INSERT INTO lots (id, product_id, lot_number, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000e101', '00000000-0000-7000-8000-00000000e001', 'L2409', 0, 0);
    INSERT INTO serials (id, product_id, serial_no, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000e111', '00000000-0000-7000-8000-00000000e001', 'SN-1', 0, 0);
    INSERT INTO stock_counts (id, location_id, scope, status, started_at, user_id)
      VALUES ('00000000-0000-7000-8000-00000000e301', '00000000-0000-7000-8000-000000000301', 'full', 'open', 1, 'u1');
    INSERT INTO stock_count_lines (count_id, product_id, expected_qty)
      VALUES ('00000000-0000-7000-8000-00000000e301', '00000000-0000-7000-8000-00000000e001', 5000);
    UPDATE stock_counts SET status = 'closed', closed_at = 5, closed_by = 'u1';
    INSERT INTO stock_transfers (document_id, from_location_id, to_location_id)
      VALUES ('00000000-0000-7000-8000-00000000e801', '00000000-0000-7000-8000-000000000301',
              '00000000-0000-7000-8000-00000000e802');
    INSERT INTO cost_history (id, product_id, cost_before, cost_after, source_type, created_at)
      VALUES ('00000000-0000-7000-8000-00000000e701', '00000000-0000-7000-8000-00000000e001', 0, 18000, 'purchase', 1);
    INSERT INTO promotions (id, name_fr, kind, campaign, value, starts_on, ends_on, created_at, updated_at)
      VALUES ('00000000-0000-7000-8000-00000000e501', 'Soldes d''hiver', 'percent', 'solde', 3000,
              '2027-01-10', '2027-02-20', 0, 0);
    INSERT INTO promotion_products (promotion_id, product_id, reference_price, created_at)
      VALUES ('00000000-0000-7000-8000-00000000e501', '00000000-0000-7000-8000-00000000e001', 25000, 0);
    INSERT INTO sale_line_promotions (line_id, document_id, promotion_id, source, amount)
      VALUES ('00000000-0000-7000-8000-00000000e901', '00000000-0000-7000-8000-00000000e902',
              '00000000-0000-7000-8000-00000000e501', 'promotion', 7500);
    INSERT INTO import_jobs (id, kind, file_name, policy, status, mapping, created_at)
      VALUES ('00000000-0000-7000-8000-00000000ea01', 'products', 'articles.xlsx', 'upsert', 'done', '[]', 0);
    INSERT INTO import_job_rows (job_id, row_no, entity_id, action)
      VALUES ('00000000-0000-7000-8000-00000000ea01', 2, '00000000-0000-7000-8000-00000000e001', 'created');";

/// The append-only and never-deleted guards of 0003, each tried on a real row: the soldes
/// evidence (`promotion_products`, `sale_line_promotions`), the ledgers and their allocations,
/// the trails (cost, imports, transfers) and the frozen counts. The trigger inventory is exact.
#[test]
fn every_back_office_trail_refuses_its_update_and_delete() {
    let db = Db::open_memory().unwrap();
    let cases: &[(&'static str, &'static str)] = &[
        ("customers", "DELETE FROM customers"),
        ("customer_ledger", "UPDATE customer_ledger SET note = 'x'"),
        ("customer_ledger", "DELETE FROM customer_ledger"),
        ("customer_allocations", "UPDATE customer_allocations SET amount = 1"),
        ("customer_allocations", "DELETE FROM customer_allocations"),
        ("suppliers", "DELETE FROM suppliers"),
        ("supplier_ledger", "UPDATE supplier_ledger SET note = 'x'"),
        ("supplier_ledger", "DELETE FROM supplier_ledger"),
        ("supplier_allocations", "UPDATE supplier_allocations SET amount = 1"),
        ("supplier_allocations", "DELETE FROM supplier_allocations"),
        ("expense_categories", "DELETE FROM expense_categories"),
        ("expenses", "UPDATE expenses SET note = 'x'"),
        ("expenses", "DELETE FROM expenses"),
        ("lots", "DELETE FROM lots"),
        ("serials", "DELETE FROM serials"),
        ("stock_counts", "UPDATE stock_counts SET note = 'x'"),
        ("stock_counts", "DELETE FROM stock_counts"),
        ("stock_count_lines", "UPDATE stock_count_lines SET counted_qty = 1"),
        ("stock_count_lines", "DELETE FROM stock_count_lines"),
        (
            "stock_count_lines",
            "INSERT INTO stock_count_lines (count_id, product_id, expected_qty)
               VALUES ('00000000-0000-7000-8000-00000000e301', '00000000-0000-7000-8000-00000000e002', 0)",
        ),
        ("stock_transfers", "UPDATE stock_transfers SET driver_name = 'x'"),
        ("stock_transfers", "DELETE FROM stock_transfers"),
        ("cost_history", "UPDATE cost_history SET cost_after = 1"),
        ("cost_history", "DELETE FROM cost_history"),
        ("promotions", "DELETE FROM promotions"),
        ("promotion_products", "UPDATE promotion_products SET reference_price = 1"),
        ("promotion_products", "DELETE FROM promotion_products"),
        ("sale_line_promotions", "UPDATE sale_line_promotions SET amount = 1"),
        ("sale_line_promotions", "DELETE FROM sale_line_promotions"),
        ("import_jobs", "DELETE FROM import_jobs"),
        ("import_job_rows", "UPDATE import_job_rows SET action = 'updated'"),
        ("import_job_rows", "DELETE FROM import_job_rows"),
    ];
    let (rows, outcomes) = db
        .dry_run(move |tx| {
            tx.execute_batch(TRAIL_ROWS)?;
            let mut rows = Vec::new();
            let mut outcomes = Vec::new();
            for (table, sql) in cases {
                let count: i64 =
                    tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
                rows.push((*table, count));
                let sp = tx.savepoint()?;
                let outcome = sp
                    .execute_batch(sql)
                    .map_err(AppError::from)
                    .err()
                    .map(|e| e.code());
                drop(sp);
                outcomes.push((*sql, outcome));
            }
            Ok((rows, outcomes))
        })
        .unwrap();
    for (table, count) in rows {
        assert!(count > 0, "{table} has its row");
    }
    for (sql, outcome) in outcomes {
        assert_eq!(outcome.as_deref(), Some("constraint_violation"), "{sql}");
    }
    let names = db
        .read(|c| {
            let mut stmt =
                c.prepare("SELECT name FROM sqlite_master WHERE type = 'trigger' ORDER BY name")?;
            let names = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(names)
        })
        .unwrap();
    assert_eq!(names, TRIGGERS);
    // The counter guards are 0004's: each accepts exactly the jump a journaled skip records.
    let guards = db
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT sql FROM sqlite_master WHERE type = 'trigger'
                   AND name IN ('counters_start_at_one', 'counters_step_by_one')",
            )?;
            let sql = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(sql)
        })
        .unwrap();
    assert_eq!(guards.len(), 2);
    assert!(guards.iter().all(|sql| sql.contains("counter_skips")));
}

const SESSION: &str =
    "INSERT INTO cash_sessions (id, terminal_id, user_id, business_date, opened_at, opening_float)
    VALUES ('00000000-0000-7000-8000-00000000c001', 't1', 'u1', '2026-09-28', 0, 0);";

/// The Z record of session c001, filed under `terminal_id`.
fn file_closure(db: &Db, terminal_id: &'static str) -> Result<(), AppError> {
    db.write(move |tx| {
        tx.execute(
            "INSERT INTO cash_closures (session_id, terminal_id, z_seq, z_number, closed_at, closed_by,
                 business_date, report, grand_total, audit_last_seq, audit_last_hash, audit_intact,
                 documents_anchor, prev_hash, hash)
             VALUES ('00000000-0000-7000-8000-00000000c001', ?1, 1, 'ZC1-000001', 9, 'u1',
                     '2026-09-28', '{}', 0, 0, ?2, 1, '[]', ?2, ?2)",
            rusqlite::params![terminal_id, "0".repeat(64)],
        )?;
        Ok(())
    })
}

#[test]
fn cash_counts_need_an_open_session_and_closures_a_closed_one_of_their_terminal() {
    let db = Db::open_memory().unwrap();
    exec(&db, SESSION).unwrap();
    exec(
        &db,
        "INSERT INTO cash_counts (session_id, denomination, count)
           VALUES ('00000000-0000-7000-8000-00000000c001', 100000, 3)",
    )
    .unwrap();
    assert_eq!(
        file_closure(&db, "t1").unwrap_err().code(),
        "constraint_violation",
        "a closure records a closed session"
    );
    exec(
        &db,
        "UPDATE cash_sessions SET status = 'closed', closed_at = 9, closed_by = 'u1', expected_cash = 300000,
           counted_cash = 300000, variance = 0, z_number = 'ZC1-000001'",
    )
    .unwrap();
    let late_count = exec(
        &db,
        "INSERT INTO cash_counts (session_id, denomination, count)
           VALUES ('00000000-0000-7000-8000-00000000c001', 50000, 1)",
    );
    assert_eq!(
        late_count.unwrap_err().code(),
        "constraint_violation",
        "the count sheet is written before the session closes"
    );
    assert_eq!(
        file_closure(&db, "t2").unwrap_err().code(),
        "constraint_violation",
        "a closure belongs to the session's own terminal"
    );
    file_closure(&db, "t1").unwrap();
    assert_eq!(count(&db, "SELECT COUNT(*) FROM cash_counts"), 1);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM cash_closures"), 1);
}

/// Writes the M1 fixture: an M1 database (schema 2) with a shop, a product in stock, an open
/// till and one validated ticket. Run once, in Task 1 Step 3, before 0003 is listed; after
/// that `Db::open` would migrate the file to schema 3 and the assertion stops it.
#[test]
#[ignore = "writes tests/fixtures/m1-first-sale.db once, before 0003 is listed"]
fn write_m1_fixture() {
    let path = m1_fixture_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);
    let db = Db::open(&path, DbOptions::default()).unwrap();
    assert_eq!(
        db.report().schema_version,
        2,
        "write the M1 fixture before listing 0003"
    );
    let shop = crate::testkit::shop_in(db, TaxRegime::Reel, PriceMode::Ttc);
    let milk = shop.product("Lait Candia 1 L", Money(15_000), &shop.tva9);
    shop.stock_in(&milk.id, 24_000);
    shop.open_till(1_000_000);
    let ctx = shop.till();
    let input = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(&milk.id, Qty(2_000))]),
        payments: vec![PaymentInput::cash(Money(50_000))],
    };
    shop.db
        .write(move |tx| create_sale(tx, &ctx, input))
        .unwrap();
    shop.db.clone().close().unwrap();
}

#[test]
fn the_m1_fixture_upgrades_in_place_after_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    std::fs::copy(m1_fixture_path(), &path).unwrap();
    let db = Db::open(&path, DbOptions::default()).unwrap();
    let report = db.report().clone();
    assert_eq!((report.schema_version, report.migrations_applied), (10, 8));
    assert!(!report.unclean_shutdown);
    let backup = report
        .pre_migration_backup
        .expect("a backup before migrating");
    let copy = Connection::open(&backup).unwrap();
    assert_eq!(schema_version(&copy).unwrap(), 2);
    assert!(integrity_ok(&copy).unwrap());
    drop(copy);
    assert!(db.integrity_check().unwrap());
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM pragma_foreign_key_check"),
        0
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM documents WHERE doc_type = 'ticket' AND status = 'validated'"
        ),
        1
    );
    assert_eq!(count(&db, "SELECT SUM(qty) FROM stock_levels"), 22_000);
    let chain = db.read(audit::verify_chain).unwrap();
    assert_eq!(chain.first_broken_seq, None);
    let documents = db.read(documents::verify_all).unwrap();
    assert!(documents.iter().all(|r| r.first_broken_seq.is_none()));
    assert_eq!(count(&db, "SELECT COUNT(*) FROM expense_categories"), 8);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM customers"), 0);
    exec(&db, CUSTOMER).unwrap();
}
