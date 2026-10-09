//! A till mirrors the main PC (spec §4.6): each page of the host's feed is applied in `seq`
//! order inside one `Db::write_replica` transaction, with SQLite triggers off, because the rows
//! are already the host's committed truth (a validated ticket arrives validated, a closed
//! session closed). Rows owned by this till are its own echoes and are skipped: the till is the
//! authority of its rows and learns the host's corrections from the replay answers (Task 7).
//! Triggers being off, this module keeps by hand what they keep: the product search index and
//! the derived caches (stock levels, lot levels, customer and supplier balances), rebuilt from
//! the till's ledgers so its own unsent sales still count.

use super::capture::{Column, columns, primary_key};
use super::catalogue::{self, DERIVED, SyncClass, TableSpec, quote};
use super::feed::{FeedPage, Op};
use crate::error::AppError;
use rusqlite::types::Value as SqlValue;
use rusqlite::{OptionalExtension, Transaction, params, params_from_iter};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub rows: usize,
    pub skipped_own: usize,
    pub deleted: usize,
    pub derived: usize,
    pub applied_seq: i64,
}

fn version_mismatch() -> AppError {
    AppError::conflict("lan_version_mismatch")
}

struct Fts {
    table: &'static str,
    index: &'static str,
    columns: &'static [&'static str],
}

const FTS: &[Fts] = &[Fts {
    table: "products",
    index: "products_fts",
    columns: &["search_text", "reference"],
}];

pub fn apply_page(
    tx: &Transaction<'_>,
    local_terminal: &str,
    page: &FeedPage,
    now: i64,
) -> Result<ApplyReport, AppError> {
    let mut report = ApplyReport::default();
    let mut schemas: HashMap<String, Vec<Column>> = HashMap::new();
    let mut touched: BTreeMap<&'static str, BTreeSet<Vec<String>>> = BTreeMap::new();
    for row in &page.rows {
        if row.owner.as_deref() == Some(local_terminal) {
            report.skipped_own += 1;
            continue;
        }
        let spec = catalogue::spec(&row.table).ok_or_else(version_mismatch)?;
        if !matches!(spec.class, SyncClass::Shared | SyncClass::TillOwned) {
            return Err(version_mismatch());
        }
        if !schemas.contains_key(&row.table) {
            if !catalogue::present(tx, spec.name)? {
                return Err(version_mismatch());
            }
            schemas.insert(row.table.clone(), columns(tx, spec.name)?);
        }
        let cols = schemas.get(&row.table).ok_or_else(version_mismatch)?;
        let fts = FTS.iter().find(|f| f.table == row.table);
        let before = match fts {
            Some(f) => fts_row(tx, f, &row.image)?,
            None => None,
        };
        match row.op {
            Op::Delete => {
                delete(tx, spec, cols, &row.image)?;
                report.deleted += 1;
            }
            Op::Insert | Op::Update => upsert(tx, spec, cols, &row.image)?,
        }
        if let Some(f) = fts {
            let after = fts_row(tx, f, &row.image)?;
            refresh_fts(tx, f, before, after)?;
        }
        note_derived(&mut touched, &row.table, &row.image);
        report.rows += 1;
    }
    report.derived = rebuild_derived(tx, &touched, now)?;
    tx.execute(
        "UPDATE lan_local SET applied_seq = MAX(applied_seq, ?1), updated_at = ?2",
        params![page.next_since, now],
    )?;
    report.applied_seq = page.next_since;
    Ok(report)
}

fn to_sql(value: &Value, col: &Column) -> Result<SqlValue, AppError> {
    Ok(match value {
        Value::Null => SqlValue::Null,
        Value::Bool(b) => SqlValue::Integer(i64::from(*b)),
        // Money, quantities, rates and times are integers: a float means another schema.
        Value::Number(n) => SqlValue::Integer(n.as_i64().ok_or_else(version_mismatch)?),
        Value::String(s) if col.is_blob() => {
            SqlValue::Blob(hex::decode(s).map_err(|_| version_mismatch())?)
        }
        Value::String(s) => SqlValue::Text(s.clone()),
        other => SqlValue::Text(other.to_string()),
    })
}

/// An image names only columns of this schema and holds the whole primary key (`keys_only`:
/// nothing else). It may lack columns: a row captured before a migration added a column to its
/// table reaches a till already updated (the column takes its default, or keeps its value).
fn check_columns(
    cols: &[Column],
    image: &Map<String, Value>,
    keys_only: bool,
) -> Result<(), AppError> {
    let pk = primary_key(cols);
    let unknown = image.keys().any(|k| !cols.iter().any(|c| &c.name == k));
    let keys_missing = pk.iter().any(|c| !image.contains_key(&c.name));
    if unknown || keys_missing || (keys_only && image.len() != pk.len()) {
        return Err(version_mismatch());
    }
    Ok(())
}

fn upsert(
    tx: &Transaction<'_>,
    spec: &TableSpec,
    cols: &[Column],
    image: &Map<String, Value>,
) -> Result<(), AppError> {
    check_columns(cols, image, false)?;
    // Only the columns the image holds: a column it lacks takes its default on insert and keeps
    // its value on update.
    let present: Vec<&Column> = cols
        .iter()
        .filter(|c| image.contains_key(&c.name))
        .collect();
    let names = present
        .iter()
        .map(|c| quote(&c.name))
        .collect::<Vec<_>>()
        .join(", ");
    let marks = (1..=present.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let pk = primary_key(cols);
    let updates = present
        .iter()
        .filter(|c| c.pk == 0 && !spec.local_columns.contains(&c.name.as_str()))
        .map(|c| format!("{0} = excluded.{0}", quote(&c.name)))
        .collect::<Vec<_>>();
    let conflict = if pk.is_empty() {
        String::new()
    } else if updates.is_empty() {
        format!(
            " ON CONFLICT ({}) DO NOTHING",
            pk.iter()
                .map(|c| quote(&c.name))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!(
            " ON CONFLICT ({}) DO UPDATE SET {}",
            pk.iter()
                .map(|c| quote(&c.name))
                .collect::<Vec<_>>()
                .join(", "),
            updates.join(", ")
        )
    };
    let sql = format!(
        "INSERT INTO {} ({names}) VALUES ({marks}){conflict}",
        quote(spec.name)
    );
    let values = present
        .iter()
        .map(|c| to_sql(image.get(&c.name).unwrap_or(&Value::Null), c))
        .collect::<Result<Vec<_>, _>>()?;
    tx.execute(&sql, params_from_iter(values))?;
    Ok(())
}

fn delete(
    tx: &Transaction<'_>,
    spec: &TableSpec,
    cols: &[Column],
    image: &Map<String, Value>,
) -> Result<(), AppError> {
    check_columns(cols, image, true)?;
    let pk = primary_key(cols);
    let clause = pk
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{} = ?{}", quote(&c.name), i + 1))
        .collect::<Vec<_>>()
        .join(" AND ");
    let values = pk
        .iter()
        .map(|c| to_sql(image.get(&c.name).unwrap_or(&Value::Null), c))
        .collect::<Result<Vec<_>, _>>()?;
    tx.execute(
        &format!("DELETE FROM {} WHERE {clause}", quote(spec.name)),
        params_from_iter(values),
    )?;
    Ok(())
}

type FtsRow = (i64, Vec<SqlValue>);

fn fts_row(
    tx: &Transaction<'_>,
    fts: &Fts,
    image: &Map<String, Value>,
) -> Result<Option<FtsRow>, AppError> {
    let Some(id) = image.get("id").and_then(Value::as_str) else {
        return Ok(None);
    };
    let cols = fts
        .columns
        .iter()
        .map(|c| quote(c))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT rowid, {cols} FROM {} WHERE \"id\" = ?1",
        quote(fts.table)
    );
    Ok(tx
        .query_row(&sql, [id], |r| {
            let mut values = Vec::with_capacity(fts.columns.len());
            for i in 0..fts.columns.len() {
                values.push(r.get::<_, SqlValue>(i + 1)?);
            }
            Ok((r.get::<_, i64>(0)?, values))
        })
        .optional()?)
}

fn refresh_fts(
    tx: &Transaction<'_>,
    fts: &Fts,
    before: Option<FtsRow>,
    after: Option<FtsRow>,
) -> Result<(), AppError> {
    let cols = fts
        .columns
        .iter()
        .map(|c| quote(c))
        .collect::<Vec<_>>()
        .join(", ");
    let marks = (0..fts.columns.len())
        .map(|i| format!("?{}", i + 2))
        .collect::<Vec<_>>()
        .join(", ");
    let index = quote(fts.index);
    if let Some((rowid, values)) = before {
        let mut bound = vec![SqlValue::Integer(rowid)];
        bound.extend(values);
        tx.execute(
            &format!("INSERT INTO {index} ({index}, rowid, {cols}) VALUES ('delete', ?1, {marks})"),
            params_from_iter(bound),
        )?;
    }
    if let Some((rowid, values)) = after {
        let mut bound = vec![SqlValue::Integer(rowid)];
        bound.extend(values);
        tx.execute(
            &format!("INSERT INTO {index} (rowid, {cols}) VALUES (?1, {marks})"),
            params_from_iter(bound),
        )?;
    }
    Ok(())
}

fn note_derived(
    touched: &mut BTreeMap<&'static str, BTreeSet<Vec<String>>>,
    table: &str,
    image: &Map<String, Value>,
) {
    for cache in DERIVED {
        let key: Option<Vec<String>> = if cache.source == table {
            cache
                .keys
                .iter()
                .map(|k| image.get(*k).and_then(Value::as_str).map(str::to_owned))
                .collect()
        } else if cache.table == table {
            image
                .get("id")
                .and_then(Value::as_str)
                .map(|id| vec![id.to_owned()])
        } else {
            None
        };
        if let Some(key) = key {
            touched.entry(cache.table).or_default().insert(key);
        }
    }
}

fn rebuild_derived(
    tx: &Transaction<'_>,
    touched: &BTreeMap<&'static str, BTreeSet<Vec<String>>>,
    now: i64,
) -> Result<usize, AppError> {
    let mut n = 0;
    for cache in DERIVED {
        let Some(keys) = touched.get(cache.table) else {
            continue;
        };
        if !catalogue::present(tx, cache.table)? || !catalogue::present(tx, cache.source)? {
            continue;
        }
        let mut stmt = tx.prepare(cache.rebuild_sql)?;
        for key in keys {
            let mut bound: Vec<SqlValue> = key.iter().cloned().map(SqlValue::Text).collect();
            bound.push(SqlValue::Integer(now));
            stmt.execute(params_from_iter(bound))?;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Actor;
    use crate::catalog::search::{SearchQuery, search};
    use crate::money::Money;
    use crate::sync::feed::{FeedPage, FeedRow, Op};
    use crate::sync::testkit::lan_pair;
    use crate::testkit::NOON;
    use serde_json::json;

    fn search_names(db: &crate::db::Db, text: &'static str) -> Vec<String> {
        db.read(move |c| {
            let query = SearchQuery {
                text: text.into(),
                limit: None,
                category_id: None,
                include_archived: false,
            };
            Ok(search(c, &query)?
                .into_iter()
                .map(|hit| hit.name_fr)
                .collect())
        })
        .unwrap()
    }

    #[test]
    fn a_till_mirrors_the_catalogue_and_finds_it_by_search() {
        let lan = lan_pair();
        let milk = lan
            .host
            .product("Lait entier", Money(10_000), &lan.host.tva19);
        lan.host.product("Pain", Money(1_000), &lan.host.tva19);
        let report = lan.pull();
        assert!(report.rows >= 2);
        assert_eq!(search_names(&lan.till, "entier"), ["Lait entier"]);
        let id = milk.id.clone();
        let actor = lan.host.owner.actor();
        lan.host
            .db
            .write(move |tx| {
                let mut input = crate::catalog::products::ProductInput::from(
                    &crate::catalog::products::get(tx, &id)?,
                );
                input.name_fr = "Lait demi-écrémé".into();
                crate::catalog::products::update(tx, &actor, &id, input)
            })
            .unwrap();
        lan.pull();
        assert!(
            search_names(&lan.till, "entier").is_empty(),
            "the old name left the index"
        );
        assert_eq!(search_names(&lan.till, "demi"), ["Lait demi-écrémé"]);
        assert_eq!(
            lan.count_till("SELECT COUNT(*) FROM products"),
            lan.count_host("SELECT COUNT(*) FROM products")
        );
    }

    #[test]
    fn other_tills_tickets_arrive_and_the_caches_count_the_tills_own_unsent_sales() {
        let lan = lan_pair();
        let milk = lan.host.product("Lait", Money(10_000), &lan.host.tva19);
        lan.host.stock_in(&milk.id, 10_000);
        lan.pull();
        lan.open_till_session(0);
        lan.sell_on_till(&milk.id, 1_000, 10_500); // unsent: this till is offline
        lan.host.open_till(0);
        let ctx = lan.host.till();
        let input = crate::sales::SaleInput {
            cart: crate::sales::Cart::new(
                crate::lang::Lang::Fr,
                vec![crate::sales::CartLine::product(
                    &milk.id,
                    crate::money::Qty(2_000),
                )],
            ),
            payments: vec![crate::payments::PaymentInput::cash(Money(20_500))],
        };
        lan.host
            .db
            .write(move |tx| crate::sales::create_sale(tx, &ctx, input))
            .unwrap();
        let report = lan.pull();
        assert!(report.derived >= 1);
        assert_eq!(
            lan.count_till(
                "SELECT COUNT(*) FROM documents WHERE series = 'C1' AND status = 'validated'"
            ),
            1,
            "the main PC's ticket arrived, with its session"
        );
        assert_eq!(
            lan.count_till("SELECT qty FROM stock_levels"),
            7_000,
            "10 in, 2 sold on C1, 1 sold here and not yet sent"
        );
        assert_eq!(lan.count_host("SELECT qty FROM stock_levels"), 8_000);
    }

    #[test]
    fn a_page_is_applied_whole_or_not_at_all() {
        let lan = lan_pair();
        lan.host.product("Lait", Money(10_000), &lan.host.tva19);
        let since: i64 = lan.count_till("SELECT applied_seq FROM lan_local");
        let me = lan.till_terminal.id.clone();
        let mut page = lan
            .host
            .db
            .read(move |c| crate::sync::feed::page(c, since, &me, 100))
            .unwrap();
        let mut broken = page.rows[0].clone();
        broken.seq += 1_000;
        broken
            .image
            .insert("column_from_the_future".into(), json!(1));
        page.rows.push(broken);
        let me = lan.till_terminal.id.clone();
        let err = lan
            .till
            .write_replica(move |tx| apply_page(tx, &me, &page, NOON))
            .unwrap_err();
        assert_eq!(err.code(), "lan_version_mismatch");
        assert_eq!(
            lan.count_till("SELECT COUNT(*) FROM products WHERE name_fr = 'Lait'"),
            0
        );
        assert_eq!(
            lan.count_till("SELECT applied_seq FROM lan_local"),
            since,
            "nothing moved"
        );
    }

    #[test]
    fn deletes_blobs_and_lockout_state() {
        let lan = lan_pair();
        let milk = lan.host.product("Lait", Money(10_000), &lan.host.tva19);
        let pid = milk.id.clone();
        lan.host
            .db
            .write(move |tx| {
                tx.execute(
                    "INSERT INTO product_barcodes (id, product_id, barcode, created_at)
                     VALUES ('00000000-0000-7000-8000-00000000b0c1', ?1, '6130000000017', 0)",
                    [&pid],
                )?;
                tx.execute("UPDATE company SET logo = X'89504E47', updated_at = 9", [])?;
                Ok(())
            })
            .unwrap();
        lan.pull();
        assert_eq!(lan.count_till("SELECT COUNT(*) FROM product_barcodes"), 1);
        assert_eq!(lan.count_till("SELECT length(logo) FROM company"), 4);
        lan.host
            .db
            .write(|tx| Ok(tx.execute("DELETE FROM product_barcodes", [])?))
            .unwrap();
        let cashier = lan.till_cashier.user_id.clone();
        lan.till
            .write(move |tx| {
                Ok(tx.execute(
                    "UPDATE users SET failed_attempts = 3 WHERE id = ?1",
                    [&cashier],
                )?)
            })
            .unwrap();
        let cashier = lan.host.cashier.user_id.clone();
        let actor = lan.host.owner.actor();
        lan.host
            .db
            .write(move |tx| {
                tx.execute(
                    "UPDATE users SET display_name = 'Yacine B.', updated_at = 9 WHERE id = ?1",
                    [&cashier],
                )?;
                crate::audit::append(
                    tx,
                    &actor.event("user.updated", "user", Some(&cashier), json!({})),
                )?;
                Ok(())
            })
            .unwrap();
        let report = lan.pull();
        assert_eq!(report.deleted, 1);
        assert_eq!(lan.count_till("SELECT COUNT(*) FROM product_barcodes"), 0);
        let (name, attempts): (String, i64) = lan
            .till
            .read(|c| {
                Ok(c.query_row(
                    "SELECT display_name, failed_attempts FROM users WHERE display_name LIKE 'Yacine%'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?)
            })
            .unwrap();
        assert_eq!(
            (name.as_str(), attempts),
            ("Yacine B.", 3),
            "the till keeps its lockout state"
        );
        let _ = Actor::default();
    }

    #[test]
    fn a_row_captured_before_a_column_was_added_still_applies() {
        let lan = lan_pair();
        // Captured on the main PC, not yet pulled by the till.
        let milk = lan.host.product("Lait", Money(10_000), &lan.host.tva19);
        lan.pull();
        let id = milk.id.clone();
        let actor = lan.host.owner.actor();
        lan.host
            .db
            .write(move |tx| {
                let mut input = crate::catalog::products::ProductInput::from(
                    &crate::catalog::products::get(tx, &id)?,
                );
                input.name_fr = "Lait entier".into();
                crate::catalog::products::update(tx, &actor, &id, input)
            })
            .unwrap();
        lan.host.product("Pain", Money(1_000), &lan.host.tva19);
        // The till is updated first: a migration added a column to a replicated table.
        lan.till
            .write_replica(|tx| {
                tx.execute_batch(
                    "ALTER TABLE products ADD COLUMN shelf_note TEXT NOT NULL DEFAULT 'none'",
                )?;
                tx.execute("UPDATE products SET shelf_note = 'kept'", [])?;
                Ok(())
            })
            .unwrap();
        lan.pull();
        let rows: Vec<(String, String)> = lan
            .till
            .read(|c| {
                let mut stmt =
                    c.prepare("SELECT name_fr, shelf_note FROM products ORDER BY name_fr")?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        assert_eq!(
            rows,
            [
                ("Lait entier".to_owned(), "kept".to_owned()),
                ("Pain".to_owned(), "none".to_owned())
            ],
            "the update keeps the new column, the insert takes its default"
        );
    }

    #[test]
    fn an_image_without_its_primary_key_stops_the_page() {
        let lan = lan_pair();
        let me = lan.till_terminal.id.clone();
        let page = FeedPage {
            rows: vec![FeedRow {
                seq: 1,
                table: "units".into(),
                row_id: "x".into(),
                op: Op::Update,
                owner: None,
                image: json!({ "code": "u", "created_at": 0, "updated_at": 0 })
                    .as_object()
                    .unwrap()
                    .clone(),
            }],
            next_since: 1,
            head: 1,
            more: false,
        };
        let err = lan
            .till
            .write_replica(move |tx| apply_page(tx, &me, &page, NOON))
            .unwrap_err();
        assert_eq!(err.code(), "lan_version_mismatch");
    }

    #[test]
    fn an_unknown_table_or_a_float_stops_the_page() {
        let lan = lan_pair();
        let me = lan.till_terminal.id.clone();
        let page = |table: &str, image: serde_json::Value| FeedPage {
            rows: vec![FeedRow {
                seq: 1,
                table: table.into(),
                row_id: "x".into(),
                op: Op::Update,
                owner: None,
                image: image.as_object().unwrap().clone(),
            }],
            next_since: 1,
            head: 1,
            more: false,
        };
        for bad in [
            page("tables_of_m9", json!({ "id": "x" })),
            page(
                "units",
                json!({ "id": "x", "code": "u", "name_ar": "", "name_fr": "", "name_en": "",
                                  "decimals": 1.5, "archived_at": null, "created_at": 0, "updated_at": 0 }),
            ),
        ] {
            let me = me.clone();
            let err = lan
                .till
                .write_replica(move |tx| apply_page(tx, &me, &bad, NOON))
                .unwrap_err();
            assert_eq!(err.code(), "lan_version_mismatch");
        }
    }
}
