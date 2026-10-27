//! What replicates on a shop LAN and who owns each row (spec §4.6). `Shared` rows are the
//! main PC's truth, mirrored by every till, written on a till only in `client_writable`
//! columns; `TillOwned` rows are made by one till for its own operations (its tickets, drawer,
//! kridi entries, counters) and `owner` says which; `Derived` caches are rebuilt from their
//! ledger on each PC and never travel; `Journal` is the audit journal (a till's rows reach the
//! host's `lan_journal`, the host's stay home); `LocalOnly` is this PC's own data; `HostOnly`
//! rows are written and kept on the main PC only (a till refuses every write with
//! `host_required` and never receives them: M8's legal registers and the main PC's own books).
//! A table the catalogue does not know fails `every_table_of_the_schema_is_classified`: every
//! later milestone classifies its tables here.

use crate::db::meta_get;
use crate::error::AppError;
use crate::terminal::META_TERMINAL_ID;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncClass {
    Shared,
    TillOwned,
    Derived,
    Journal,
    LocalOnly,
    HostOnly,
}

/// `(SELECT column FROM parent WHERE id = row.fk)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParentRef {
    pub fk: &'static str,
    pub parent: &'static str,
    pub column: &'static str,
}

impl ParentRef {
    fn sql(&self, row: &str) -> String {
        format!(
            "(SELECT {} FROM {} WHERE \"id\" = {row}.{})",
            quote(self.column),
            quote(self.parent),
            quote(self.fk)
        )
    }
}

/// How a row names the till that owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// Host data.
    None,
    /// The row's own `terminal_id`-like column: the terminal that made it (a row made on the
    /// main PC names the main PC's own terminal).
    Column(&'static str),
    /// The owner of its parent row.
    Parent {
        fk: &'static str,
        parent: &'static str,
        column: &'static str,
    },
    /// Either of two parents (an allocation links a debit and a credit entry, often made on
    /// different PCs): the row is a till's when either parent is (`guard_sql`,
    /// `owners_of_image`). As one name (`sql`, `owner_of_image`): this PC's terminal when it
    /// owns either parent, else the parents' common owner, else NULL. So the till that made an
    /// allocation captures it as its own, and the main PC never names a till that may lack it.
    Either { first: ParentRef, second: ParentRef },
    /// A counter's (or counter skip's) `series` is a terminal code for per-till series, empty
    /// for the company's.
    Series,
}

/// This PC's own terminal id (`app_meta`), for generated SQL. The key is a constant.
fn local_terminal_sql() -> String {
    format!("(SELECT \"value\" FROM \"app_meta\" WHERE \"key\" = '{META_TERMINAL_ID}')")
}

impl Owner {
    /// The SQL naming the terminal that owns `row` (`NEW`/`OLD`) on this PC: the capture's
    /// `_t`. `None` for host data.
    pub fn sql(&self, row: &str) -> Option<String> {
        match self {
            Owner::None => None,
            Owner::Column(column) => Some(format!("{row}.{}", quote(column))),
            Owner::Parent { fk, parent, column } => Some(ParentRef { fk, parent, column }.sql(row)),
            Owner::Either { first, second } => {
                let (first, second, local) =
                    (first.sql(row), second.sql(row), local_terminal_sql());
                Some(format!(
                    "CASE WHEN {local} IN ({first}, {second}) THEN {local} \
                     WHEN {first} IS {second} THEN {first} END"
                ))
            }
            Owner::Series => Some(format!(
                "(SELECT \"id\" FROM \"terminals\" WHERE \"code\" = {row}.\"series\")"
            )),
        }
    }

    /// True when the row is **not** owned by `terminal_id` (the guard's condition). The id is
    /// inlined as a literal: callers pass only ids that satisfy `ids::is_valid_id`.
    pub fn guard_sql(&self, row: &str, terminal_id: &str) -> Option<String> {
        match self {
            Owner::None => None,
            Owner::Either { first, second } => Some(format!(
                "({} IS NOT '{terminal_id}' AND {} IS NOT '{terminal_id}')",
                first.sql(row),
                second.sql(row)
            )),
            other => other
                .sql(row)
                .map(|owner| format!("{owner} IS NOT '{terminal_id}'")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSpec {
    pub name: &'static str,
    pub class: SyncClass,
    pub owner: Owner,
    /// Columns a till keeps when a host image of the row arrives (per-PC state).
    pub local_columns: &'static [&'static str],
    /// Columns of a `Shared` table a till may write itself (per-PC state, caches).
    pub client_writable: &'static [&'static str],
    /// A `TillOwned` table where a till may only write rows whose `column` is one of `values`.
    pub till_only: Option<(&'static str, &'static [&'static str])>,
}

const fn shared(name: &'static str) -> TableSpec {
    TableSpec {
        name,
        class: SyncClass::Shared,
        owner: Owner::None,
        local_columns: &[],
        client_writable: &[],
        till_only: None,
    }
}

const fn till(name: &'static str, owner: Owner) -> TableSpec {
    TableSpec {
        name,
        class: SyncClass::TillOwned,
        owner,
        local_columns: &[],
        client_writable: &[],
        till_only: None,
    }
}

const fn of_document(name: &'static str, fk: &'static str) -> TableSpec {
    till(
        name,
        Owner::Parent {
            fk,
            parent: "documents",
            column: "terminal_id",
        },
    )
}

const fn of_session(name: &'static str) -> TableSpec {
    till(
        name,
        Owner::Parent {
            fk: "session_id",
            parent: "cash_sessions",
            column: "terminal_id",
        },
    )
}

const fn with_class(name: &'static str, class: SyncClass) -> TableSpec {
    TableSpec {
        name,
        class,
        owner: Owner::None,
        local_columns: &[],
        client_writable: &[],
        till_only: None,
    }
}

const TERMINAL: Owner = Owner::Column("terminal_id");

const fn of_parent(name: &'static str, fk: &'static str, parent: &'static str) -> TableSpec {
    till(
        name,
        Owner::Parent {
            fk,
            parent,
            column: "terminal_id",
        },
    )
}

/// Every table of M0–M8. A table absent from the schema is skipped.
pub const TABLES: &[TableSpec] = &[
    // M0 foundation
    with_class("app_meta", SyncClass::LocalOnly),
    shared("settings"),
    shared("company"),
    shared("terminals"),
    shared("roles"),
    TableSpec {
        // A till keeps its own lockout state and each user's language (the AR/FR/EN toggle,
        // which also stamps `updated_at`); the main PC's value seeds a user new to the till.
        local_columns: &["failed_attempts", "locked_until", "last_login_at", "lang"],
        client_writable: &[
            "failed_attempts",
            "locked_until",
            "last_login_at",
            "lang",
            "updated_at",
        ],
        ..shared("users")
    },
    with_class("audit_log", SyncClass::Journal),
    with_class("backups_log", SyncClass::LocalOnly),
    // M1 first sale
    shared("categories"),
    shared("units"),
    shared("tax_rates"),
    shared("price_levels"),
    shared("products"),
    shared("product_barcodes"),
    shared("product_prices"),
    shared("product_components"),
    with_class("products_fts", SyncClass::LocalOnly),
    shared("stock_locations"),
    with_class("stock_levels", SyncClass::Derived),
    till("stock_movements", TERMINAL),
    till("counters", Owner::Series),
    till("cash_sessions", TERMINAL),
    of_session("cash_movements"),
    of_session("cash_counts"),
    till("cash_closures", TERMINAL),
    TableSpec {
        till_only: Some(("doc_type", &["ticket", "return_note"])),
        ..till("documents", TERMINAL)
    },
    of_document("document_lines", "document_id"),
    of_document("document_taxes", "document_id"),
    till("payments", TERMINAL),
    with_class("held_carts", SyncClass::LocalOnly),
    // M2 back office
    TableSpec {
        client_writable: &["balance", "updated_at"],
        ..shared("customers")
    },
    till("customer_ledger", TERMINAL),
    till(
        "customer_allocations",
        Owner::Either {
            first: ParentRef {
                fk: "credit_id",
                parent: "customer_ledger",
                column: "terminal_id",
            },
            second: ParentRef {
                fk: "debit_id",
                parent: "customer_ledger",
                column: "terminal_id",
            },
        },
    ),
    shared("suppliers"),
    shared("supplier_ledger"),
    shared("supplier_allocations"),
    shared("expense_categories"),
    till("expenses", TERMINAL),
    shared("stock_counts"),
    shared("stock_count_lines"),
    // 0009 (STK-004): a count's lot lines, shared like the count and its product lines.
    shared("stock_count_lot_lines"),
    shared("lots"),
    with_class("lot_levels", SyncClass::Derived),
    shared("serials"),
    shared("promotions"),
    shared("promotion_products"),
    of_document("sale_line_promotions", "document_id"),
    shared("price_breaks"),
    shared("product_extras"),
    shared("location_minimums"),
    shared("stock_transfers"),
    shared("cost_history"),
    with_class("import_jobs", SyncClass::LocalOnly),
    with_class("import_job_rows", SyncClass::LocalOnly),
    // M3 documents and fiscal
    // The numbers a restore made a counter jump over: they travel with the counter, since the
    // counter triggers let it jump only over a recorded skip.
    till("counter_skips", Owner::Series),
    of_document("document_parties", "document_id"),
    of_document("document_links", "to_id"),
    of_document("document_drafts", "document_id"),
    // M4 hardware
    // Each weighed product's hotkey, tare and shelf life: catalogue data every till's scale
    // panel reads, set on the main PC like the product itself.
    shared("scale_items"),
    // The last shelf label printed per product, the shop's one reprint queue (LBL-007): shelf
    // labels print on the main PC; a till's weighed labels are journaled only, never kept here.
    shared("label_prints"),
    // M5 LAN (and M7's change feed)
    with_class("changelog", SyncClass::LocalOnly),
    with_class("sync_cursors", SyncClass::LocalOnly),
    with_class("lan_local", SyncClass::LocalOnly),
    with_class("lan_outbox", SyncClass::LocalOnly),
    with_class("lan_devices", SyncClass::LocalOnly),
    with_class("lan_inbox", SyncClass::LocalOnly),
    with_class("lan_journal", SyncClass::LocalOnly),
    with_class("lan_commits", SyncClass::LocalOnly),
    with_class("lan_notices", SyncClass::LocalOnly),
    // M7 DZBuild link: runs on the main PC only
    with_class("cloud_links", SyncClass::LocalOnly),
    with_class("cloud_map", SyncClass::LocalOnly),
    with_class("sync_outbox", SyncClass::LocalOnly),
    with_class("cloud_stock", SyncClass::LocalOnly),
    with_class("cloud_publish", SyncClass::LocalOnly),
    with_class("cloud_inbox", SyncClass::LocalOnly),
    with_class("cloud_endpoints", SyncClass::LocalOnly),
    with_class("cloud_notices", SyncClass::LocalOnly),
    with_class("online_orders", SyncClass::LocalOnly),
    with_class("online_order_lines", SyncClass::LocalOnly),
    with_class("remote_customer_flags", SyncClass::LocalOnly),
    with_class("cloud_backup_keys", SyncClass::LocalOnly),
    with_class("cloud_backups", SyncClass::LocalOnly),
    with_class("cloud_variant_map", SyncClass::LocalOnly),
    // M8 vertical packs, from the `-- LAN:` intent above each table of 0008_packs.sql, under
    // Every balance-bearing ledger (the flexy float, deposits, vouchers,
    // salary advances, instalment contracts) is written on the main PC only (its commands are
    // host services; a till offline refuses them with `host_required`) and mirrored to the
    // tills, so they are `Shared` here; a customer's points are earned by the till that sells
    // (`till_only` = `earn`) and spent, expired, reversed or adjusted on the main PC. The two
    // legal registers numbered shop-wide by (year, seq), the delivery rounds and the zakat
    // statements are `HostOnly`: never on a till.
    shared("open_price_products"),
    shared("department_keys"),
    shared("product_regulations"),
    shared("dining_rooms"),
    shared("dining_tables"),
    till("dining_tickets", TERMINAL),
    shared("modifier_groups"),
    shared("modifier_options"),
    shared("product_modifier_groups"),
    of_parent("dining_lines", "ticket_id", "dining_tickets"),
    shared("kitchen_stations"),
    shared("category_stations"),
    till("kitchen_tickets", TERMINAL),
    shared("daily_menu"),
    shared("pharmacy_products"),
    shared("chifa_prices"),
    // Health data (loi 18-07 art. 18): kept on the main PC, never mirrored to a till.
    with_class("controlled_register", SyncClass::HostOnly),
    shared("yield_templates"),
    shared("yield_template_cuts"),
    till("yield_runs", TERMINAL),
    of_parent("yield_run_cuts", "run_id", "yield_runs"),
    shared("flexy_float"),
    till("service_tickets", TERMINAL),
    of_parent("service_ticket_events", "ticket_id", "service_tickets"),
    of_parent("service_ticket_items", "ticket_id", "service_tickets"),
    shared("deposits"),
    shared("instalment_plans"),
    shared("instalment_dues"),
    shared("instalment_guarantees"),
    shared("instalment_events"),
    shared("customer_prices"),
    shared("product_wholesale"),
    with_class("delivery_rounds", SyncClass::HostOnly),
    with_class("delivery_round_stops", SyncClass::HostOnly),
    of_document("customer_orders", "document_id"),
    of_document("order_events", "document_id"),
    of_document("stock_reservations", "owner_id"),
    shared("appointments"),
    shared("loyalty_cards"),
    shared("gift_vouchers"),
    shared("gift_voucher_ledger"),
    TableSpec {
        till_only: Some(("kind", &["earn"])),
        ..till("loyalty_ledger", TERMINAL)
    },
    shared("jewellery_pieces"),
    shared("metal_prices"),
    // The jeweller's register (CII art. 359) holds the buyers' identity documents.
    with_class("jeweller_register", SyncClass::HostOnly),
    shared("staff_profiles"),
    shared("staff_ledger"),
    with_class("zakat_statements", SyncClass::HostOnly),
    shared("payment_types"),
    of_parent("payment_type_uses", "payment_id", "payments"),
    shared("customer_consents"),
];

/// Settings that belong to one PC (theme, the login screen's language, UX-041).
pub const PER_PC_SETTINGS: [&str; 2] = ["ui.theme", "ui.login_lang"];

/// A cache rebuilt from its ledger for the keys a feed page touched (triggers are off while a
/// till applies a page). `rebuild_sql` binds the keys in order, then `?{n+1}` = now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivedCache {
    pub table: &'static str,
    pub source: &'static str,
    pub keys: &'static [&'static str],
    pub rebuild_sql: &'static str,
}

pub const DERIVED: &[DerivedCache] = &[
    DerivedCache {
        table: "stock_levels",
        source: "stock_movements",
        keys: &["product_id", "location_id"],
        rebuild_sql: "INSERT INTO stock_levels (product_id, location_id, qty, updated_at)
             SELECT ?1, ?2, COALESCE(SUM(qty_delta), 0), ?3 FROM stock_movements
             WHERE product_id = ?1 AND location_id = ?2
             ON CONFLICT (product_id, location_id) DO UPDATE SET qty = excluded.qty, updated_at = excluded.updated_at",
    },
    DerivedCache {
        table: "lot_levels",
        source: "stock_movements",
        keys: &["lot_id", "location_id"],
        rebuild_sql: "INSERT INTO lot_levels (lot_id, location_id, qty, updated_at)
             SELECT ?1, ?2, COALESCE(SUM(qty_delta), 0), ?3 FROM stock_movements
             WHERE lot_id = ?1 AND location_id = ?2
             ON CONFLICT (lot_id, location_id) DO UPDATE SET qty = excluded.qty, updated_at = excluded.updated_at",
    },
    DerivedCache {
        table: "customers",
        source: "customer_ledger",
        keys: &["customer_id"],
        rebuild_sql: "UPDATE customers SET balance =
             (SELECT COALESCE(SUM(amount), 0) FROM customer_ledger WHERE customer_id = ?1)
             WHERE id = ?1 AND ?2 IS NOT NULL",
    },
    DerivedCache {
        table: "suppliers",
        source: "supplier_ledger",
        keys: &["supplier_id"],
        rebuild_sql: "UPDATE suppliers SET balance =
             (SELECT COALESCE(SUM(amount), 0) FROM supplier_ledger WHERE supplier_id = ?1)
             WHERE id = ?1 AND ?2 IS NOT NULL",
    },
];

/// `"name"`, with embedded quotes doubled.
pub fn quote(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

pub fn spec(name: &str) -> Option<&'static TableSpec> {
    TABLES.iter().find(|t| t.name == name)
}

pub fn class_of(name: &str) -> Option<SyncClass> {
    if name == "sqlite_sequence" || name.starts_with("products_fts_") {
        return Some(SyncClass::LocalOnly);
    }
    spec(name).map(|t| t.class)
}

pub fn schema_tables(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'
         ORDER BY name",
    )?;
    let names = stmt
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(names)
}

pub fn unclassified(conn: &Connection) -> Result<Vec<String>, AppError> {
    Ok(schema_tables(conn)?
        .into_iter()
        .filter(|name| class_of(name).is_none())
        .collect())
}

pub fn present(conn: &Connection, name: &str) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [name],
        |r| r.get(0),
    )?)
}

/// The `Shared` and `TillOwned` tables of this schema, in catalogue order.
pub fn replicated(conn: &Connection) -> Result<Vec<&'static TableSpec>, AppError> {
    let mut out = Vec::new();
    for spec in TABLES {
        if matches!(spec.class, SyncClass::Shared | SyncClass::TillOwned)
            && present(conn, spec.name)?
        {
            out.push(spec);
        }
    }
    Ok(out)
}

fn lookup(
    conn: &Connection,
    parent: &ParentRef,
    image: &Map<String, Value>,
) -> Result<Option<String>, AppError> {
    let Some(key) = image.get(parent.fk).and_then(Value::as_str) else {
        return Ok(None);
    };
    let sql = format!(
        "SELECT {} FROM {} WHERE \"id\" = ?1",
        quote(parent.column),
        quote(parent.parent)
    );
    Ok(conn
        .query_row(&sql, [key], |r| r.get::<_, Option<String>>(0))
        .optional()?
        .flatten())
}

/// The terminal owning a row image as this PC names it (`Owner::sql`, the capture's `_t`), read
/// from this database (the parent may have arrived earlier in the same replayed item). `None`:
/// host data, or an `Either` row between two other tills. Whether a till may send a row is a
/// membership question: `owners_of_image`.
pub fn owner_of_image(
    conn: &Connection,
    spec: &TableSpec,
    image: &Map<String, Value>,
) -> Result<Option<String>, AppError> {
    match spec.owner {
        Owner::None => Ok(None),
        Owner::Column(column) => Ok(image.get(column).and_then(Value::as_str).map(str::to_owned)),
        Owner::Parent { fk, parent, column } => {
            lookup(conn, &ParentRef { fk, parent, column }, image)
        }
        Owner::Either { first, second } => {
            let (first, second) = (lookup(conn, &first, image)?, lookup(conn, &second, image)?);
            let owns =
                |local: &String| first.as_ref() == Some(local) || second.as_ref() == Some(local);
            Ok(match meta_get(conn, META_TERMINAL_ID)? {
                Some(local) if owns(&local) => Some(local),
                _ if first == second => first,
                _ => None,
            })
        }
        Owner::Series => {
            let Some(series) = image.get("series").and_then(Value::as_str) else {
                return Ok(None);
            };
            Ok(conn
                .query_row("SELECT id FROM terminals WHERE code = ?1", [series], |r| {
                    r.get(0)
                })
                .optional()?)
        }
    }
}

/// Every terminal that owns a row image, read from this database: the one owner of most rows,
/// both parents' owners of an `Either` row (an allocation is a till's when either of its entries
/// is, as its guard has it), none for host data. The main PC accepts a replayed row only when
/// the sending till is one of them.
pub fn owners_of_image(
    conn: &Connection,
    spec: &TableSpec,
    image: &Map<String, Value>,
) -> Result<Vec<String>, AppError> {
    let Owner::Either { first, second } = spec.owner else {
        return Ok(owner_of_image(conn, spec, image)?.into_iter().collect());
    };
    let mut owners: Vec<String> = [lookup(conn, &first, image)?, lookup(conn, &second, image)?]
        .into_iter()
        .flatten()
        .collect();
    owners.dedup();
    Ok(owners)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use serde_json::json;

    #[test]
    fn every_table_of_the_schema_is_classified() {
        let db = Db::open_memory().unwrap();
        let missing = db.read(unclassified).unwrap();
        assert!(
            missing.is_empty(),
            "classify these tables in sync::catalogue::TABLES: {missing:?}"
        );
        let names: std::collections::BTreeSet<&str> = TABLES.iter().map(|t| t.name).collect();
        assert_eq!(names.len(), TABLES.len(), "one entry per table");
    }

    #[test]
    fn the_classes_follow_the_lan_rules() {
        assert_eq!(class_of("products"), Some(SyncClass::Shared));
        assert_eq!(class_of("documents"), Some(SyncClass::TillOwned));
        assert_eq!(class_of("stock_levels"), Some(SyncClass::Derived));
        assert_eq!(class_of("audit_log"), Some(SyncClass::Journal));
        assert_eq!(class_of("held_carts"), Some(SyncClass::LocalOnly));
        assert_eq!(class_of("lan_outbox"), Some(SyncClass::LocalOnly));
        assert_eq!(class_of("products_fts_data"), Some(SyncClass::LocalOnly));
        assert_eq!(class_of("sqlite_sequence"), Some(SyncClass::LocalOnly));
        assert_eq!(class_of("nonsense"), None);
        // M8: balances are host writes mirrored to the tills, registers stay home.
        assert_eq!(class_of("flexy_float"), Some(SyncClass::Shared));
        assert_eq!(class_of("gift_voucher_ledger"), Some(SyncClass::Shared));
        assert_eq!(class_of("staff_ledger"), Some(SyncClass::Shared));
        assert_eq!(class_of("deposits"), Some(SyncClass::Shared));
        assert_eq!(class_of("instalment_plans"), Some(SyncClass::Shared));
        assert_eq!(class_of("controlled_register"), Some(SyncClass::HostOnly));
        assert_eq!(class_of("jeweller_register"), Some(SyncClass::HostOnly));
        assert_eq!(class_of("delivery_rounds"), Some(SyncClass::HostOnly));
        assert_eq!(class_of("zakat_statements"), Some(SyncClass::HostOnly));
        assert_eq!(class_of("dining_tickets"), Some(SyncClass::TillOwned));
        assert_eq!(
            spec("loyalty_ledger").unwrap().till_only,
            Some(("kind", &["earn"][..])),
            "a till earns points; spending them is the main PC's"
        );
        assert_eq!(PER_PC_SETTINGS, ["ui.theme", "ui.login_lang"]);
        let users = spec("users").unwrap();
        assert_eq!(
            users.local_columns,
            ["failed_attempts", "locked_until", "last_login_at", "lang"]
        );
        assert_eq!(
            users.client_writable,
            [
                "failed_attempts",
                "locked_until",
                "last_login_at",
                "lang",
                "updated_at"
            ],
            "set_language stamps updated_at with lang"
        );
        assert_eq!(
            spec("customers").unwrap().client_writable,
            ["balance", "updated_at"]
        );
        assert_eq!(
            spec("documents").unwrap().till_only,
            Some(("doc_type", &["ticket", "return_note"][..]))
        );
    }

    #[test]
    fn owners_name_the_till_of_a_row() {
        assert_eq!(spec("products").unwrap().owner.sql("NEW"), None);
        assert_eq!(
            spec("documents").unwrap().owner.sql("NEW").unwrap(),
            "NEW.\"terminal_id\""
        );
        assert_eq!(
            spec("document_lines").unwrap().owner.sql("OLD").unwrap(),
            "(SELECT \"terminal_id\" FROM \"documents\" WHERE \"id\" = OLD.\"document_id\")"
        );
        assert_eq!(
            spec("counters").unwrap().owner.sql("NEW").unwrap(),
            "(SELECT \"id\" FROM \"terminals\" WHERE \"code\" = NEW.\"series\")"
        );
        let allocation = spec("customer_allocations").unwrap();
        let (credit, debit, local) = (
            "(SELECT \"terminal_id\" FROM \"customer_ledger\" WHERE \"id\" = NEW.\"credit_id\")",
            "(SELECT \"terminal_id\" FROM \"customer_ledger\" WHERE \"id\" = NEW.\"debit_id\")",
            "(SELECT \"value\" FROM \"app_meta\" WHERE \"key\" = 'terminal_id')",
        );
        assert_eq!(
            allocation.owner.sql("NEW").unwrap(),
            format!(
                "CASE WHEN {local} IN ({credit}, {debit}) THEN {local} \
                 WHEN {credit} IS {debit} THEN {credit} END"
            ),
            "this PC when it made either entry, else their common owner"
        );
        let t = "00000000-0000-7000-8000-0000000000c2";
        assert_eq!(
            allocation.owner.guard_sql("NEW", t).unwrap(),
            format!(
                "((SELECT \"terminal_id\" FROM \"customer_ledger\" WHERE \"id\" = NEW.\"credit_id\") IS NOT '{t}' \
                 AND (SELECT \"terminal_id\" FROM \"customer_ledger\" WHERE \"id\" = NEW.\"debit_id\") IS NOT '{t}')"
            ),
            "an allocation is the till's when either of its entries is"
        );
        assert_eq!(quote("we\"ird"), "\"we\"\"ird\"");
    }

    #[test]
    fn the_owner_of_an_image_is_read_from_the_database() {
        let db = Db::open_memory().unwrap();
        db.write(|tx| {
            tx.execute_batch(
                "INSERT INTO terminals (id, code, name, role, created_at)
                   VALUES ('00000000-0000-7000-8000-0000000000c2', 'C2', 'Caisse 2', 'client', 0);
                 INSERT INTO documents (id, doc_type, series, business_date, user_id, terminal_id, created_at, updated_at)
                   VALUES ('00000000-0000-7000-8000-00000000f001', 'ticket', 'C2', '2026-09-28', 'u',
                           '00000000-0000-7000-8000-0000000000c2', 0, 0);",
            )?;
            Ok(())
        })
        .unwrap();
        let owner = |table: &'static str, image: serde_json::Value| {
            db.read(move |c| {
                let spec = spec(table).unwrap();
                owner_of_image(c, spec, image.as_object().unwrap())
            })
            .unwrap()
        };
        let c2 = Some("00000000-0000-7000-8000-0000000000c2".to_owned());
        assert_eq!(
            owner(
                "document_lines",
                json!({ "document_id": "00000000-0000-7000-8000-00000000f001" })
            ),
            c2
        );
        assert_eq!(owner("counters", json!({ "series": "C2" })), c2);
        assert_eq!(
            owner("counters", json!({ "series": "" })),
            None,
            "company series: the host's"
        );
        assert_eq!(owner("products", json!({ "id": "p" })), None);
    }

    #[test]
    fn derived_caches_rebuild_from_their_ledgers() {
        for cache in DERIVED {
            assert!(class_of(cache.source).is_some(), "{}", cache.source);
            assert!(cache.rebuild_sql.contains(cache.source), "{}", cache.table);
        }
        assert_eq!(
            DERIVED.iter().map(|d| d.table).collect::<Vec<_>>(),
            ["stock_levels", "lot_levels", "customers", "suppliers"]
        );
    }
}
