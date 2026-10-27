//! The shop's whole data as text (IMX-016, loi 18-07 access and portability): every table the
//! shop owns, never a secret (PIN hashes) or a picture. The full export writes one CSV per
//! table; the import side of a migration is the `.dzpos` backup.

use crate::error::AppError;
use rusqlite::Connection;
use rusqlite::types::ValueRef;

pub const DATASET: &[&str] = &[
    "company",
    "settings",
    "terminals",
    "roles",
    "users",
    "audit_log",
    "backups_log",
    "categories",
    "units",
    "tax_rates",
    "price_levels",
    "products",
    "product_barcodes",
    "product_prices",
    "product_components",
    "product_extras",
    "price_breaks",
    "scale_items",
    "label_prints",
    "stock_locations",
    "stock_levels",
    "stock_movements",
    "lots",
    "lot_levels",
    "serials",
    "stock_counts",
    "stock_count_lines",
    "stock_count_lot_lines",
    "stock_transfers",
    "location_minimums",
    "cost_history",
    "counters",
    "counter_skips",
    "cash_sessions",
    "cash_movements",
    "cash_counts",
    "cash_closures",
    "documents",
    "document_lines",
    "document_taxes",
    "document_parties",
    "document_links",
    "document_drafts",
    "payments",
    "held_carts",
    "customers",
    "customer_ledger",
    "customer_allocations",
    "suppliers",
    "supplier_ledger",
    "supplier_allocations",
    "expense_categories",
    "expenses",
    "promotions",
    "promotion_products",
    "sale_line_promotions",
    "import_jobs",
    "import_job_rows",
    "cloud_links",
    "cloud_publish",
    "online_orders",
    "online_order_lines",
    // The tills' audit journals as the main PC received them: part of the shop's record.
    "lan_journal",
    // What the main PC noticed taking the tills' operations (refusals, oversold stock…).
    "lan_notices",
    // M8 vertical packs. LAN intent, as 0008_packs.sql states it above each table: `shared` is
    // host data every till mirrors, `till(col)` a row owned by the till its `col` names,
    // `parent(fk -> table)` a row owned like its parent, `host-only` written and kept on the
    // main PC.
    "open_price_products",     // shared
    "department_keys",         // shared
    "product_regulations",     // shared
    "dining_rooms",            // shared
    "dining_tables",           // shared
    "dining_tickets",          // till(terminal_id)
    "modifier_groups",         // shared
    "modifier_options",        // shared
    "product_modifier_groups", // shared
    "dining_lines",            // parent(ticket_id ->dining_tickets)
    "kitchen_stations",        // shared
    "category_stations",       // shared
    "kitchen_tickets",         // till(terminal_id)
    "daily_menu",              // shared
    "pharmacy_products",       // shared
    "chifa_prices",            // shared
    "yield_templates",         // shared
    "yield_template_cuts",     // shared
    "yield_runs",              // till(terminal_id)
    "yield_run_cuts",          // parent(run_id ->yield_runs)
    "flexy_float",             // till(terminal_id)
    "service_tickets",         // till(terminal_id)
    "service_ticket_events",   // parent(ticket_id ->service_tickets)
    "service_ticket_items",    // parent(ticket_id ->service_tickets)
    "deposits",                // till(terminal_id)
    "instalment_plans",        // till(terminal_id)
    "instalment_dues",         // parent(plan_id ->instalment_plans)
    "instalment_guarantees",   // parent(plan_id ->instalment_plans)
    "instalment_events",       // parent(plan_id ->instalment_plans)
    "customer_prices",         // shared
    "product_wholesale",       // shared
    "delivery_rounds",         // host-only
    "delivery_round_stops",    // host-only
    "customer_orders",         // parent(document_id ->documents)
    "order_events",            // parent(document_id ->documents)
    "stock_reservations",      // parent(owner_id ->documents)
    "appointments",            // shared
    "loyalty_cards",           // shared
    "gift_vouchers",           // till(terminal_id)
    "gift_voucher_ledger",     // till(terminal_id)
    "loyalty_ledger",          // till(terminal_id)
    "jewellery_pieces",        // shared
    "metal_prices",            // shared
    "jeweller_register",       // till(terminal_id)
    "staff_profiles",          // shared
    "staff_ledger",            // till(terminal_id)
    "zakat_statements",        // host-only
    "payment_types",           // shared
    "payment_type_uses",       // parent(payment_id ->payments)
    "customer_consents",       // shared
];

/// Tables left out on purpose: the schema and sync bookkeeping (the change feed, this PC's LAN
/// state, the till's outbox, the main PC's replay inbox and commit marks, the paired
/// tills with their token hashes), DZBuild's own buyer flags, the cloud backup tables of 0.7–0.9
/// (a passphrase-wrapped secret; the feature is gone since 1.0.0, the tables stay), and the controlled-medicines register, the one table of health
/// data (loi 18-07 art. 18, PDP-009): it leaves the shop only through its own report, read with
/// `pharmacy.register`.
pub const SKIPPED_TABLES: &[&str] = &[
    "app_meta",
    "changelog",
    "sync_cursors",
    "lan_local",
    "lan_outbox",
    "lan_devices",
    "lan_inbox",
    "lan_commits",
    "cloud_map",
    "cloud_variant_map",
    "sync_outbox",
    "cloud_stock",
    "cloud_inbox",
    "cloud_endpoints",
    "cloud_notices",
    "remote_customer_flags",
    "cloud_backup_keys",
    "cloud_backups",
    "controlled_register",
];

pub const SECRET_COLUMNS: &[(&str, &str)] = &[("users", "pin_hash"), ("company", "logo")];

/// The exported columns of `table`, in table order.
pub fn columns(conn: &Connection, table: &str) -> Result<Vec<String>, AppError> {
    let Some(table) = DATASET.iter().find(|t| **t == table) else {
        return Err(AppError::validation("table", "value_invalid"));
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT name FROM pragma_table_info('{table}') ORDER BY cid"
    ))?;
    let names = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(names
        .into_iter()
        .filter(|c| !SECRET_COLUMNS.contains(&(*table, c.as_str())))
        .collect())
}

/// Calls `visit` with every row of `table` as text (integers in full, NULL as empty) and returns
/// the column names.
pub fn dump(
    conn: &Connection,
    table: &str,
    visit: &mut dyn FnMut(&[String]) -> Result<(), AppError>,
) -> Result<Vec<String>, AppError> {
    let names = columns(conn, table)?;
    let list = names
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let mut stmt = conn.prepare(&format!("SELECT {list} FROM \"{table}\""))?;
    let mut rows = stmt.query([])?;
    let mut cells = Vec::with_capacity(names.len());
    while let Some(row) = rows.next()? {
        cells.clear();
        for i in 0..names.len() {
            cells.push(match row.get_ref(i)? {
                ValueRef::Null | ValueRef::Blob(_) => String::new(),
                ValueRef::Integer(v) => v.to_string(),
                ValueRef::Real(v) => v.to_string(),
                ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
            });
        }
        visit(&cells)?;
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::shop;

    #[test]
    fn the_full_export_covers_every_table_and_no_secret() {
        let shop = shop();
        let tables: Vec<String> = shop
            .db
            .read(|c| {
                let mut stmt = c.prepare(
                    "SELECT name FROM sqlite_master WHERE type = 'table'
                       AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'products_fts%' ORDER BY name",
                )?;
                let names = stmt
                    .query_map([], |r| r.get(0))?
                    .collect::<Result<Vec<String>, _>>()?;
                Ok(names)
            })
            .unwrap();
        let mut expected: Vec<String> = DATASET
            .iter()
            .chain(SKIPPED_TABLES.iter())
            .map(|t| (*t).to_owned())
            .collect();
        expected.sort();
        assert_eq!(
            tables, expected,
            "a new table must be exported or explicitly skipped"
        );
        let (columns, rows) = shop
            .db
            .read(|c| {
                let mut rows = Vec::new();
                let columns = dump(c, "users", &mut |r| {
                    rows.push(r.to_vec());
                    Ok(())
                })?;
                Ok((columns, rows))
            })
            .unwrap();
        assert!(!columns.iter().any(|c| c == "pin_hash"));
        assert!(rows.len() >= 2 && rows.iter().all(|r| r.len() == columns.len()));
        assert!(
            !shop
                .db
                .read(|c| dump(c, "company", &mut |_| Ok(())))
                .unwrap()
                .iter()
                .any(|c| c == "logo")
        );
        assert_eq!(
            shop.db
                .read(|c| dump(c, "sqlite_master", &mut |_| Ok(())))
                .unwrap_err(),
            AppError::validation("table", "value_invalid")
        );
    }
}
