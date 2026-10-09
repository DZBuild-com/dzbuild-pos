//! Triggers generated from the live schema (M5): capture triggers write the full image of each
//! replicated row into `changelog` while a reader is registered, guard triggers stop a till from
//! writing the main PC's data (`host_required`). Generated at run time, not in a migration, so a
//! single PC carries none and every column a later migration adds is captured: `ensure` compares
//! the statements' signature with the one recorded in `app_meta` at each start, and the triggers
//! in the schema with the statements (a rebuilt table lost its triggers with the same columns).
//!
//! Identifiers come from the catalogue and `PRAGMA table_xinfo` and are quoted; the literals
//! inlined are the catalogue's constants (per-PC settings keys, a till's document types,
//! `app_meta`'s terminal key) and the till's own terminal id, which must be a UUIDv7
//! (`ids::is_valid_id`).

use super::catalogue::{self, Owner, PER_PC_SETTINGS, SyncClass, TABLES, TableSpec, quote};
use crate::db::{meta_get, meta_set};
use crate::error::{AppError, HOST_REQUIRED_RAISE};
use crate::ids::is_valid_id;
use rusqlite::{Connection, Transaction};
use sha2::{Digest, Sha256};

pub const CAPTURE_PREFIX: &str = "lan_cap_";
pub const GUARD_PREFIX: &str = "lan_guard_";
const META_SIGNATURE: &str = "lan_triggers_sig";
/// Columns per `json_object`/`json_insert` call (two arguments each), well under SQLite's limit.
const CHUNK: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureSet {
    /// The main PC: shared and till-owned rows, for the tills.
    Host,
    /// A till: its own rows and journal for its outbox, and guards against host-only writes.
    Client { terminal_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub decl_type: String,
    /// Position in the primary key (1-based), 0 when not part of it.
    pub pk: i64,
}

impl Column {
    pub fn is_blob(&self) -> bool {
        self.decl_type.eq_ignore_ascii_case("BLOB")
    }
}

/// The stored columns of `table` (generated and hidden columns left out).
pub fn columns(conn: &Connection, table: &str) -> Result<Vec<Column>, AppError> {
    let mut stmt =
        conn.prepare("SELECT name, type, pk, hidden FROM pragma_table_xinfo(?1) ORDER BY cid")?;
    let cols = stmt
        .query_map([table], |r| {
            Ok((
                Column {
                    name: r.get(0)?,
                    decl_type: r.get(1)?,
                    pk: r.get(2)?,
                },
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(cols
        .into_iter()
        .filter(|(_, hidden)| *hidden == 0)
        .map(|(c, _)| c)
        .collect())
}

pub fn primary_key(cols: &[Column]) -> Vec<&Column> {
    let mut pk: Vec<&Column> = cols.iter().filter(|c| c.pk > 0).collect();
    pk.sort_by_key(|c| c.pk);
    pk
}

fn value_sql(row: &str, col: &Column) -> String {
    let field = format!("{row}.{}", quote(&col.name));
    if col.is_blob() {
        format!("CASE WHEN {field} IS NULL THEN NULL ELSE hex({field}) END")
    } else {
        field
    }
}

/// `json_object('_lan', 1, '_t', owner, 'c1', row.c1, …)`, continued with `json_insert` beyond
/// `CHUNK` columns (json_patch would drop the nulls).
fn image_sql(row: &str, owner: &Owner, cols: &[&Column]) -> String {
    let owner = owner.sql(row).unwrap_or_else(|| "NULL".into());
    let mut chunks = cols.chunks(CHUNK);
    let first = chunks.next().unwrap_or(&[]);
    let mut sql = format!(
        "json_object('_lan', 1, '_t', {owner}{})",
        first
            .iter()
            .map(|c| format!(", '{}', {}", c.name, value_sql(row, c)))
            .collect::<String>()
    );
    for chunk in chunks {
        sql = format!(
            "json_insert({sql}{})",
            chunk
                .iter()
                .map(|c| format!(", '$.{}', {}", c.name, value_sql(row, c)))
                .collect::<String>()
        );
    }
    sql
}

fn row_id_sql(row: &str, cols: &[Column]) -> String {
    let pk = primary_key(cols);
    if pk.is_empty() {
        return format!("CAST({row}.rowid AS TEXT)");
    }
    pk.iter()
        .map(|c| format!("CAST({row}.{} AS TEXT)", quote(&c.name)))
        .collect::<Vec<_>>()
        .join(" || '|' || ")
}

fn settings_filter(spec: &TableSpec, row: &str) -> Option<String> {
    (spec.name == "settings").then(|| {
        let keys = PER_PC_SETTINGS
            .iter()
            .map(|k| format!("'{k}'"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{row}.\"key\" NOT IN ({keys})")
    })
}

const HAS_READER: &str = "EXISTS (SELECT 1 FROM \"sync_cursors\" WHERE \"kind\" = 'changelog')";

fn capture(spec: &TableSpec, cols: &[Column], op: &str) -> (String, String) {
    let row = if op == "delete" { "OLD" } else { "NEW" };
    let image_cols: Vec<&Column> = if op == "delete" {
        primary_key(cols)
    } else {
        cols.iter().collect()
    };
    let mut when = HAS_READER.to_owned();
    if let Some(filter) = settings_filter(spec, row) {
        when = format!("{when} AND {filter}");
    }
    let name = format!("{CAPTURE_PREFIX}{}_{op}", spec.name);
    let sql = format!(
        "CREATE TRIGGER {} AFTER {} ON {} WHEN {when}
BEGIN
  INSERT INTO \"changelog\" (\"at\", \"table_name\", \"row_id\", \"op\", \"payload\")
  VALUES (CAST(unixepoch('subsec') * 1000 AS INTEGER), '{}', {}, '{op}', {});
END",
        quote(&name),
        op.to_uppercase(),
        quote(spec.name),
        spec.name,
        row_id_sql(row, cols),
        image_sql(row, &spec.owner, &image_cols),
    );
    (name, sql)
}

fn guard(name: String, timing: &str, table: &str, when: Option<String>) -> (String, String) {
    let when = when.map(|w| format!(" WHEN {w}")).unwrap_or_default();
    let sql = format!(
        "CREATE TRIGGER {} BEFORE {timing} ON {}{when}
BEGIN
  SELECT RAISE(ABORT, '{HOST_REQUIRED_RAISE}');
END",
        quote(&name),
        quote(table)
    );
    (name, sql)
}

fn shared_guards(spec: &TableSpec, cols: &[Column]) -> Vec<(String, String)> {
    let pk = primary_key(cols);
    let exists = pk
        .iter()
        .map(|c| format!("{} = NEW.{}", quote(&c.name), quote(&c.name)))
        .collect::<Vec<_>>()
        .join(" AND ");
    let new_filter = settings_filter(spec, "NEW");
    let old_filter = settings_filter(spec, "OLD");
    let mut insert_when = format!(
        "NOT EXISTS (SELECT 1 FROM {} WHERE {exists})",
        quote(spec.name)
    );
    if let Some(filter) = &new_filter {
        insert_when = format!("{insert_when} AND {filter}");
    }
    let guarded: Vec<&Column> = cols
        .iter()
        .filter(|c| !spec.client_writable.contains(&c.name.as_str()))
        .collect();
    let update_timing = if spec.client_writable.is_empty() {
        "UPDATE".to_owned()
    } else {
        format!(
            "UPDATE OF {}",
            guarded
                .iter()
                .map(|c| quote(&c.name))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    vec![
        guard(
            format!("{GUARD_PREFIX}{}_insert", spec.name),
            "INSERT",
            spec.name,
            Some(insert_when),
        ),
        guard(
            format!("{GUARD_PREFIX}{}_update", spec.name),
            &update_timing,
            spec.name,
            new_filter,
        ),
        guard(
            format!("{GUARD_PREFIX}{}_delete", spec.name),
            "DELETE",
            spec.name,
            old_filter,
        ),
    ]
}

fn till_guards(spec: &TableSpec, terminal_id: &str) -> Vec<(String, String)> {
    let not_mine = |row: &str| {
        spec.owner
            .guard_sql(row, terminal_id)
            .unwrap_or_else(|| "1".into())
    };
    let outside = |row: &str| {
        spec.till_only.map(|(column, values)| {
            let list = values
                .iter()
                .map(|v| format!("'{v}'"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{row}.{} NOT IN ({list})", quote(column))
        })
    };
    let with_outside = |base: String, row: &str| match outside(row) {
        Some(extra) => format!("{base} OR {extra}"),
        None => base,
    };
    vec![
        guard(
            format!("{GUARD_PREFIX}{}_insert", spec.name),
            "INSERT",
            spec.name,
            Some(with_outside(not_mine("NEW"), "NEW")),
        ),
        guard(
            format!("{GUARD_PREFIX}{}_update", spec.name),
            "UPDATE",
            spec.name,
            Some(with_outside(
                format!("{} OR {}", not_mine("OLD"), not_mine("NEW")),
                "NEW",
            )),
        ),
        guard(
            format!("{GUARD_PREFIX}{}_delete", spec.name),
            "DELETE",
            spec.name,
            Some(not_mine("OLD")),
        ),
    ]
}

/// A `HostOnly` table on a till: every write is the main PC's (`host_required`).
fn host_only_guards(spec: &TableSpec) -> Vec<(String, String)> {
    ["insert", "update", "delete"]
        .into_iter()
        .map(|op| {
            guard(
                format!("{GUARD_PREFIX}{}_{op}", spec.name),
                &op.to_uppercase(),
                spec.name,
                None,
            )
        })
        .collect()
}

/// Every trigger of `set` for this schema, in catalogue order.
pub fn statements(conn: &Connection, set: &CaptureSet) -> Result<Vec<(String, String)>, AppError> {
    if let CaptureSet::Client { terminal_id } = set
        && !is_valid_id(terminal_id)
    {
        return Err(AppError::validation("terminalId", "value_invalid"));
    }
    let mut out = Vec::new();
    for spec in TABLES {
        if !catalogue::present(conn, spec.name)? {
            continue;
        }
        let cols = columns(conn, spec.name)?;
        match (set, spec.class) {
            (CaptureSet::Host, SyncClass::Shared | SyncClass::TillOwned) => {
                for op in ["insert", "update", "delete"] {
                    out.push(capture(spec, &cols, op));
                }
            }
            (CaptureSet::Client { .. }, SyncClass::Journal) => {
                out.push(capture(spec, &cols, "insert"))
            }
            (CaptureSet::Client { terminal_id }, SyncClass::TillOwned) => {
                for op in ["insert", "update", "delete"] {
                    out.push(capture(spec, &cols, op));
                }
                out.extend(till_guards(spec, terminal_id));
            }
            (CaptureSet::Client { .. }, SyncClass::Shared) => {
                out.extend(shared_guards(spec, &cols))
            }
            (CaptureSet::Client { .. }, SyncClass::HostOnly) => out.extend(host_only_guards(spec)),
            _ => {}
        }
    }
    Ok(out)
}

fn digest(statements: &[(String, String)]) -> String {
    let mut hasher = Sha256::new();
    for (_, sql) in statements {
        hasher.update(sql.as_bytes());
        hasher.update([0x1e_u8]);
    }
    hex::encode(hasher.finalize().as_slice())
}

pub fn signature(conn: &Connection, set: &CaptureSet) -> Result<String, AppError> {
    Ok(digest(&statements(conn, set)?))
}

/// The LAN triggers in the schema with their SQL as SQLite keeps it, sorted by name.
fn installed_sql(conn: &Connection) -> Result<Vec<(String, String)>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT name, sql FROM sqlite_master WHERE type = 'trigger'
           AND (name LIKE 'lan\\_cap\\_%' ESCAPE '\\' OR name LIKE 'lan\\_guard\\_%' ESCAPE '\\')
         ORDER BY name",
    )?;
    let triggers = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(triggers)
}

pub fn installed(conn: &Connection) -> Result<Vec<String>, AppError> {
    Ok(installed_sql(conn)?
        .into_iter()
        .map(|(name, _)| name)
        .collect())
}

pub fn uninstall(tx: &Transaction<'_>) -> Result<usize, AppError> {
    let names = installed(tx)?;
    for name in &names {
        tx.execute_batch(&format!("DROP TRIGGER IF EXISTS {}", quote(name)))?;
    }
    tx.execute("DELETE FROM app_meta WHERE key = ?1", [META_SIGNATURE])?;
    Ok(names.len())
}

/// Replaces every LAN trigger with the ones of `set` and records their signature.
pub fn install(tx: &Transaction<'_>, set: &CaptureSet) -> Result<usize, AppError> {
    let statements = statements(tx, set)?;
    replace(tx, &statements)
}

fn replace(tx: &Transaction<'_>, statements: &[(String, String)]) -> Result<usize, AppError> {
    uninstall(tx)?;
    for (_, sql) in statements {
        tx.execute_batch(sql)?;
    }
    meta_set(tx, META_SIGNATURE, &digest(statements))?;
    Ok(statements.len())
}

/// Reinstalls when the schema (or the set) changed since the last install, or when the LAN
/// triggers in the schema are not exactly the generated ones: DROP TABLE takes a table's triggers
/// with it, so a table a migration rebuilt with the same columns keeps the signature and loses
/// its capture and guard triggers. `true` when it reinstalled.
pub fn ensure(tx: &Transaction<'_>, set: &CaptureSet) -> Result<bool, AppError> {
    let wanted = statements(tx, set)?;
    let mut sorted = wanted.clone();
    sorted.sort();
    if meta_get(tx, META_SIGNATURE)?.as_deref() == Some(digest(&wanted).as_str())
        && installed_sql(tx)? == sorted
    {
        return Ok(false);
    }
    replace(tx, &wanted)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Actor;
    use crate::db::Db;
    use crate::lang::Lang;
    use crate::money::{Money, Qty};
    use crate::payments::PaymentInput;
    use crate::sales::{Cart, CartLine, SaleInput, create_sale};
    use crate::sync::catalogue::replicated;
    use crate::testkit::{NOON, shop};
    use serde_json::{Value, json};

    const CONSUMER: &str = "INSERT INTO sync_cursors (name, kind, last_seq, updated_at)
        VALUES ('changelog:lan:test', 'changelog', 0, 0)";

    /// The LAN's rows of the change feed: M7's routing triggers (0006) write to the same
    /// `changelog` while any reader is registered, and M5's readers skip every row without `_lan`.
    fn feed(db: &Db) -> Vec<(String, String, Value)> {
        db.read(|c| {
            let mut stmt =
                c.prepare("SELECT table_name, op, payload FROM changelog ORDER BY seq")?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap()
        .into_iter()
        .map(|(t, op, p)| (t, op, serde_json::from_str::<Value>(&p).unwrap()))
        .filter(|(_, _, payload)| payload.get("_lan").is_some())
        .collect()
    }

    fn install_set(db: &Db, set: CaptureSet) -> usize {
        db.write(move |tx| install(tx, &set)).unwrap()
    }

    #[test]
    fn a_single_pc_has_no_lan_trigger() {
        let db = Db::open_memory().unwrap();
        assert!(db.read(installed).unwrap().is_empty());
    }

    #[test]
    fn the_host_captures_full_images_only_while_a_consumer_exists() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let n = install_set(&shop.db, CaptureSet::Host);
        assert!(n > 0);
        let id = milk.id.clone();
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| {
                crate::catalog::products::set_price(
                    tx,
                    &actor,
                    &id,
                    crate::catalog::RETAIL_LEVEL_ID,
                    Money(11_000),
                )
            })
            .unwrap();
        assert!(feed(&shop.db).is_empty(), "no consumer, no feed");
        shop.db.write(|tx| Ok(tx.execute_batch(CONSUMER)?)).unwrap();
        let id = milk.id.clone();
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| {
                crate::catalog::products::set_price(
                    tx,
                    &actor,
                    &id,
                    crate::catalog::RETAIL_LEVEL_ID,
                    Money(12_000),
                )
            })
            .unwrap();
        shop.set("stock.negative_policy", json!("block"));
        shop.set("ui.theme", json!("dark"));
        let rows = feed(&shop.db);
        let tables: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
        assert!(tables.contains(&"products"), "{tables:?}");
        assert!(tables.contains(&"settings"));
        assert!(
            !tables.contains(&"audit_log"),
            "the host's journal stays home"
        );
        let product = rows.iter().find(|r| r.0 == "products").unwrap();
        assert_eq!(product.2["_lan"], 1);
        assert!(product.2["_t"].is_null(), "host data");
        let cols = shop.db.read(|c| columns(c, "products")).unwrap();
        for col in &cols {
            assert!(
                product.2.get(&col.name).is_some(),
                "the image has {}",
                col.name
            );
        }
        let settings: Vec<&Value> = rows
            .iter()
            .filter(|r| r.0 == "settings")
            .map(|r| &r.2)
            .collect();
        assert_eq!(settings.len(), 1, "ui.theme belongs to this PC");
        assert_eq!(settings[0]["key"], "stock.negative_policy");
    }

    #[test]
    fn blobs_travel_as_hex_and_deletes_carry_the_key() {
        let shop = shop();
        install_set(&shop.db, CaptureSet::Host);
        shop.db.write(|tx| Ok(tx.execute_batch(CONSUMER)?)).unwrap();
        shop.db
            .write(|tx| {
                tx.execute("UPDATE company SET logo = X'0102FF', updated_at = 5", [])?;
                Ok(())
            })
            .unwrap();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let pid = milk.id.clone();
        shop.db
            .write(move |tx| {
                tx.execute(
                    "INSERT INTO product_barcodes (id, product_id, barcode, created_at)
                     VALUES ('00000000-0000-7000-8000-00000000b0c1', ?1, '6130000000017', 0)",
                    [&pid],
                )?;
                tx.execute(
                    "DELETE FROM product_barcodes WHERE barcode = '6130000000017'",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        let rows = feed(&shop.db);
        let company = rows.iter().find(|r| r.0 == "company").unwrap();
        assert_eq!(company.2["logo"], "0102FF");
        let delete = rows
            .iter()
            .find(|r| r.0 == "product_barcodes" && r.1 == "delete")
            .unwrap();
        assert_eq!(delete.2["id"], "00000000-0000-7000-8000-00000000b0c1");
        assert!(
            delete.2.get("barcode").is_none(),
            "a delete carries the key only"
        );
    }

    #[test]
    fn a_till_captures_its_own_rows_and_journal_and_refuses_host_writes() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(0);
        let me = shop.terminal.id.clone();
        install_set(
            &shop.db,
            CaptureSet::Client {
                terminal_id: me.clone(),
            },
        );
        shop.db.write(|tx| Ok(tx.execute_batch(CONSUMER)?)).unwrap();

        let ctx = shop.till();
        let input = SaleInput {
            cart: Cart::new(Lang::Fr, vec![CartLine::product(&milk.id, Qty(1_000))]),
            payments: vec![PaymentInput::cash(Money(10_500))],
        };
        shop.db
            .write(move |tx| create_sale(tx, &ctx, input))
            .unwrap();
        let rows = feed(&shop.db);
        let tables: std::collections::BTreeSet<&str> = rows.iter().map(|r| r.0.as_str()).collect();
        for t in [
            "documents",
            "document_lines",
            "document_taxes",
            "payments",
            "stock_movements",
            "cash_movements",
            "counters",
            "audit_log",
        ] {
            assert!(tables.contains(t), "{t} captured: {tables:?}");
        }
        assert!(!tables.contains("stock_levels"), "caches never travel");
        let foreign: Vec<_> = rows
            .iter()
            .filter(|r| r.0 != "audit_log" && r.2["_t"] != me.as_str())
            .collect();
        assert!(foreign.is_empty(), "every row names this till: {foreign:?}");

        let refused = |sql: &'static str| {
            shop.db
                .write(move |tx| Ok(tx.execute_batch(sql)?))
                .unwrap_err()
                .code()
        };
        let id = milk.id.clone();
        let actor = shop.owner.actor();
        let price = shop
            .db
            .write(move |tx| {
                crate::catalog::products::set_price(
                    tx,
                    &actor,
                    &id,
                    crate::catalog::RETAIL_LEVEL_ID,
                    Money(1),
                )
            })
            .unwrap_err();
        assert_eq!(
            price.code(),
            "host_required",
            "catalogue edits need the main PC"
        );
        let setting = shop
            .db
            .write(|tx| {
                crate::settings::set_value(
                    tx,
                    &Actor::default(),
                    "stock.negative_policy",
                    json!("allow"),
                )
            })
            .unwrap_err();
        assert_eq!(setting.code(), "host_required");
        shop.db
            .write(|tx| {
                crate::settings::set_value(tx, &Actor::default(), "ui.theme", json!("dark"))
            })
            .unwrap();
        assert_eq!(
            refused("UPDATE users SET display_name = 'X'"),
            "host_required"
        );
        shop.db
            .write(|tx| {
                Ok(tx.execute_batch("UPDATE users SET failed_attempts = 1, last_login_at = 5")?)
            })
            .unwrap();
        assert_eq!(
            refused(
                "INSERT INTO counters (doc_type, series, fiscal_year, last_number) VALUES ('invoice', '', 2026, 1)"
            ),
            "host_required",
            "company series are the host's"
        );
        assert_eq!(
            refused(
                "INSERT INTO documents (id, doc_type, series, business_date, user_id, terminal_id, created_at, updated_at)
                 VALUES ('00000000-0000-7000-8000-00000000f0f0', 'ticket', 'C9', '2026-09-28', 'u',
                         '00000000-0000-7000-8000-0000000000c9', 0, 0)"
            ),
            "host_required",
            "another till's rows"
        );
        let invoice = format!(
            "INSERT INTO documents (id, doc_type, series, business_date, user_id, terminal_id, created_at, updated_at)
             VALUES ('00000000-0000-7000-8000-00000000f0f1', 'invoice', '', '2026-09-28', 'u', '{me}', 0, 0)"
        );
        let err = shop
            .db
            .write(move |tx| Ok(tx.execute_batch(&invoice)?))
            .unwrap_err();
        assert_eq!(
            err.code(),
            "host_required",
            "invoices are made on the main PC"
        );
    }

    /// A till earns points with its sales, but every balance-bearing pack ledger and
    /// every `HostOnly` table (registers, rounds, zakat) is the main PC's to write.
    #[test]
    fn a_till_earns_points_but_refuses_the_main_pcs_pack_writes() {
        let shop = shop();
        let karim = shop.customer("Karim Bensalah");
        let me = shop.terminal.id.clone();
        install_set(
            &shop.db,
            CaptureSet::Client {
                terminal_id: me.clone(),
            },
        );
        let ledger = |kind: &str, points: i64, n: u32| {
            format!(
                "INSERT INTO loyalty_ledger (id, customer_id, kind, points, business_date, created_at, user_id, terminal_id)
                 VALUES ('00000000-0000-7000-8000-00000000e{n:03}', '{}', '{kind}', {points}, '2026-09-28', 0, 'u', '{me}')",
                karim.id
            )
        };
        let run = |sql: String| shop.db.write(move |tx| Ok(tx.execute_batch(&sql)?));
        run(ledger("earn", 12, 1)).unwrap();
        assert_eq!(
            run(ledger("adjust", 5, 2)).unwrap_err().code(),
            "host_required",
            "only earning is a till's"
        );
        for sql in [
            format!(
                "INSERT INTO flexy_float (id, operator, kind, amount, business_date, created_at, user_id, terminal_id)
                 VALUES ('00000000-0000-7000-8000-00000000e010', 'mobilis', 'purchase', 100000, '2026-09-28', 0, 'u', '{me}')"
            ),
            "INSERT INTO staff_ledger (id) VALUES ('00000000-0000-7000-8000-00000000e011')".to_owned(),
            "INSERT INTO controlled_register (id) VALUES ('00000000-0000-7000-8000-00000000e012')".to_owned(),
            "INSERT INTO jeweller_register (id) VALUES ('00000000-0000-7000-8000-00000000e013')".to_owned(),
            "INSERT INTO zakat_statements (id) VALUES ('00000000-0000-7000-8000-00000000e014')".to_owned(),
            "INSERT INTO delivery_rounds (id) VALUES ('00000000-0000-7000-8000-00000000e015')".to_owned(),
        ] {
            assert_eq!(run(sql.clone()).unwrap_err().code(), "host_required", "{sql}");
        }
        let host = shop.db.read(|c| statements(c, &CaptureSet::Host)).unwrap();
        for table in [
            "controlled_register",
            "jeweller_register",
            "zakat_statements",
        ] {
            assert!(
                !host.iter().any(|(name, _)| name.contains(table)),
                "{table} never travels"
            );
        }
    }

    #[test]
    fn every_column_of_every_replicated_table_is_captured() {
        let db = Db::open_memory().unwrap();
        let (specs, host) = db
            .read(|c| Ok((replicated(c)?, statements(c, &CaptureSet::Host)?)))
            .unwrap();
        for spec in specs {
            let (_, sql) = host
                .iter()
                .find(|(name, _)| name == &format!("lan_cap_{}_insert", spec.name))
                .unwrap_or_else(|| panic!("no insert capture for {}", spec.name));
            let cols = db.read(move |c| columns(c, spec.name)).unwrap();
            for col in cols {
                assert!(
                    sql.contains(&format!("'{}'", col.name)),
                    "{}.{}",
                    spec.name,
                    col.name
                );
            }
        }
    }

    #[test]
    fn triggers_follow_the_schema_and_uninstall_leaves_the_rest() {
        let db = Db::open_memory().unwrap();
        let set = CaptureSet::Host;
        install_set(&db, set.clone());
        let again = db.write(|tx| ensure(tx, &CaptureSet::Host)).unwrap();
        assert!(!again, "same schema, nothing to do");
        db.write(|tx| Ok(tx.execute_batch("ALTER TABLE products ADD COLUMN shelf TEXT")?))
            .unwrap();
        assert!(
            db.write(|tx| ensure(tx, &CaptureSet::Host)).unwrap(),
            "a new column is captured"
        );
        let sql: String = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT sql FROM sqlite_master WHERE name = 'lan_cap_products_update'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert!(sql.contains("'shelf'"));
        let before: i64 = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        let removed = db.write(|tx| uninstall(tx)).unwrap();
        let after: i64 = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(before - after, i64::try_from(removed).unwrap());
        assert!(db.read(installed).unwrap().is_empty());
        let bad = db.write(|tx| {
            install(
                tx,
                &CaptureSet::Client {
                    terminal_id: "C2'; DROP TABLE x; --".into(),
                },
            )
        });
        assert_eq!(
            bad.unwrap_err().code(),
            "value_invalid",
            "only a real terminal id is inlined"
        );
        let _ = NOON;
    }
}
