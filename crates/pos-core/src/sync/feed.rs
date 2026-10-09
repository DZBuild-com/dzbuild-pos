//! The main PC's change feed as tills read it (spec §4.6): full row images written by the
//! capture triggers, in `seq` order, in pages that end where a host transaction committed
//! (`lan_commits`), so a till applies whole host transactions. A till never gets its own rows
//! back (they are echoes of what it sent) and never sees M7's routing rows.

use super::changelog::{self, ChangeOp, lan_consumer};
use crate::db::WriteHook;
use crate::error::AppError;
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

pub const PAGE_ROWS: usize = 2_000;
pub const HARD_ROWS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Insert,
    Update,
    Delete,
}

impl From<ChangeOp> for Op {
    fn from(op: ChangeOp) -> Op {
        match op {
            ChangeOp::Insert => Op::Insert,
            ChangeOp::Update => Op::Update,
            ChangeOp::Delete => Op::Delete,
        }
    }
}

impl Op {
    pub const fn as_str(self) -> &'static str {
        match self {
            Op::Insert => "insert",
            Op::Update => "update",
            Op::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedRow {
    pub seq: i64,
    pub table: String,
    pub row_id: String,
    pub op: Op,
    /// The till owning the row (`_t`), `None` for host data.
    pub owner: Option<String>,
    pub image: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedPage {
    pub rows: Vec<FeedRow>,
    /// The till has everything up to here once it applied this page.
    pub next_since: i64,
    pub head: i64,
    pub more: bool,
}

pub fn head(conn: &Connection) -> Result<i64, AppError> {
    changelog::max_seq(conn)
}

pub fn register_till(tx: &Transaction<'_>, terminal_id: &str, now: i64) -> Result<i64, AppError> {
    changelog::register(tx, &lan_consumer(terminal_id), now)
}

pub fn unregister_till(tx: &Transaction<'_>, terminal_id: &str) -> Result<(), AppError> {
    changelog::unregister(tx, &lan_consumer(terminal_id))
}

/// The till confirmed it holds everything up to `since`: move its reader, then prune.
pub fn acknowledge(
    tx: &Transaction<'_>,
    terminal_id: &str,
    since: i64,
    now: i64,
) -> Result<usize, AppError> {
    changelog::advance(tx, &lan_consumer(terminal_id), since, now)?;
    prune(tx)
}

pub fn prune(tx: &Transaction<'_>) -> Result<usize, AppError> {
    let rows = changelog::prune(tx)?;
    tx.execute(
        "DELETE FROM lan_commits WHERE last_seq <= COALESCE(
             (SELECT MIN(last_seq) FROM sync_cursors WHERE kind = 'changelog'), 9223372036854775807)",
        [],
    )?;
    Ok(rows)
}

pub fn page(
    conn: &Connection,
    since: i64,
    for_terminal: &str,
    limit: usize,
) -> Result<FeedPage, AppError> {
    page_bounded(conn, since, for_terminal, limit, HARD_ROWS)
}

fn count_between(conn: &Connection, since: i64, to: i64) -> Result<usize, AppError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM changelog WHERE seq > ?1 AND seq <= ?2",
        params![since, to],
        |r| r.get(0),
    )?;
    Ok(usize::try_from(n).unwrap_or(usize::MAX))
}

pub(crate) fn page_bounded(
    conn: &Connection,
    since: i64,
    for_terminal: &str,
    limit: usize,
    hard: usize,
) -> Result<FeedPage, AppError> {
    let limit = limit.clamp(1, hard.max(1));
    let head = head(conn)?;
    let mut rows = changelog::read_after(conn, since, limit + 1)?;
    let cut = if rows.len() <= limit {
        rows.last().map_or(since, |r| r.seq)
    } else {
        let boundary = rows[limit - 1].seq;
        let inside: Option<i64> = conn.query_row(
            "SELECT MAX(last_seq) FROM lan_commits WHERE last_seq > ?1 AND last_seq <= ?2",
            params![since, boundary],
            |r| r.get(0),
        )?;
        match inside {
            Some(mark) => mark,
            None => {
                let next: Option<i64> = conn.query_row(
                    "SELECT MIN(last_seq) FROM lan_commits WHERE last_seq > ?1",
                    [boundary],
                    |r| r.get(0),
                )?;
                match next {
                    Some(mark) if count_between(conn, since, mark)? <= hard => mark,
                    // One host transaction bigger than `hard` rows (a bulk edit): split it. Rows
                    // keep their statement order, so every foreign key still points backwards.
                    _ => boundary,
                }
            }
        }
    };
    if rows.last().is_some_and(|r| r.seq < cut) {
        rows = changelog::read_after(conn, since, hard)?;
    }
    let mut out = Vec::new();
    for row in rows.into_iter().filter(|r| r.seq <= cut) {
        let Value::Object(mut image) = row.payload else {
            continue;
        };
        if image.remove("_lan").is_none() {
            continue; // a routing row of another reader (M7)
        }
        let owner = match image.remove("_t") {
            Some(Value::String(t)) => Some(t),
            _ => None,
        };
        if owner.as_deref() == Some(for_terminal) {
            continue; // the asking till's own row: an echo
        }
        out.push(FeedRow {
            seq: row.seq,
            table: row.table_name,
            row_id: row.row_id,
            op: row.op.into(),
            owner,
            image,
        });
    }
    Ok(FeedPage {
        rows: out,
        next_since: cut,
        head,
        more: cut < head,
    })
}

/// Marks the end of this transaction in the feed when it wrote feed rows after the last mark.
pub fn mark_commit(tx: &Transaction<'_>) -> Result<Option<i64>, AppError> {
    let last_mark: i64 = tx.query_row(
        "SELECT COALESCE(MAX(last_seq), 0) FROM lan_commits",
        [],
        |r| r.get(0),
    )?;
    let last_row: Option<i64> = tx.query_row(
        "SELECT MAX(seq) FROM changelog WHERE seq > ?1",
        [last_mark],
        |r| r.get(0),
    )?;
    let Some(last_row) = last_row else {
        return Ok(None);
    };
    tx.execute("INSERT INTO lan_commits (last_seq) VALUES (?1)", [last_row])?;
    Ok(Some(last_row))
}

/// The main PC's write hook: a commit mark in each transaction, the new head after `COMMIT`
/// (the server pokes the tills' WebSockets with it).
pub struct HostHook {
    pending: AtomicI64,
    on_commit: Arc<dyn Fn(i64) + Send + Sync>,
}

impl HostHook {
    pub fn new(on_commit: Arc<dyn Fn(i64) + Send + Sync>) -> HostHook {
        HostHook {
            pending: AtomicI64::new(0),
            on_commit,
        }
    }
}

impl WriteHook for HostHook {
    fn before_commit(&self, tx: &Transaction<'_>) -> Result<(), AppError> {
        if let Some(head) = mark_commit(tx)? {
            self.pending.fetch_max(head, Ordering::SeqCst);
        }
        Ok(())
    }

    fn after_commit(&self) {
        let head = self.pending.swap(0, Ordering::SeqCst);
        if head > 0 {
            (self.on_commit)(head);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Actor;
    use crate::db::Db;
    use crate::money::Money;
    use crate::sync::capture::{CaptureSet, install};
    use crate::testkit::shop;
    use std::sync::Mutex;

    const C2: &str = "00000000-0000-7000-8000-0000000000c2";
    const C3: &str = "00000000-0000-7000-8000-0000000000c3";

    fn host() -> (crate::testkit::Shop, Arc<Mutex<Vec<i64>>>) {
        let shop = shop();
        shop.db
            .write(|tx| {
                install(tx, &CaptureSet::Host)?;
                tx.execute_batch(&format!(
                    "INSERT INTO terminals (id, code, name, role, created_at) VALUES ('{C2}', 'C2', 'Caisse 2', 'client', 0);
                     INSERT INTO terminals (id, code, name, role, created_at) VALUES ('{C3}', 'C3', 'Caisse 3', 'client', 0);"
                ))?;
                register_till(tx, C2, 0)?;
                register_till(tx, C3, 0)?;
                Ok(())
            })
            .unwrap();
        let heads = Arc::new(Mutex::new(Vec::new()));
        let seen = heads.clone();
        shop.db
            .set_write_hook(Some(Arc::new(HostHook::new(Arc::new(move |h| {
                seen.lock().unwrap().push(h);
            })))));
        (shop, heads)
    }

    fn rename_three(shop: &crate::testkit::Shop) {
        let names = ["Lait", "Pain", "Sucre"];
        let rate = shop.tva19.clone();
        shop.db
            .write(move |tx| {
                for name in names {
                    crate::catalog::products::create(
                        tx,
                        &Actor::system(None),
                        crate::catalog::products::ProductInput::simple(
                            name,
                            crate::catalog::UNIT_PIECE_ID,
                            &rate,
                            Money(1_000),
                        ),
                    )?;
                }
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn pages_end_at_commits_skip_routing_rows_and_the_askers_own_rows() {
        let (shop, heads) = host();
        let start = shop.db.read(head).unwrap();
        rename_three(&shop); // one transaction, several feed rows
        shop.product("Huile", Money(50_000), &shop.tva19); // a second one
        shop.db
            .write(|tx| {
                // A routing row of M7's cloud reader: never sent to a till.
                tx.execute(
                    "INSERT INTO changelog (at, table_name, row_id, op, payload) VALUES (1, 'products', 'x', 'update', '{}')",
                    [],
                )?;
                // A ticket of C2 recorded on the host (a replay): C2 must not get it back.
                tx.execute_batch(&format!(
                    "INSERT INTO documents (id, doc_type, series, business_date, user_id, terminal_id, created_at, updated_at)
                     VALUES ('00000000-0000-7000-8000-00000000f002', 'ticket', 'C2', '2026-09-28', 'u', '{C2}', 0, 0)"
                ))?;
                Ok(())
            })
            .unwrap();
        let marks: i64 = shop
            .db
            .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM lan_commits", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(marks, 3, "one mark per committed transaction that fed rows");
        assert_eq!(
            heads.lock().unwrap().len(),
            3,
            "the hook reported each new head"
        );

        let first = shop.db.read(move |c| page(c, start, C2, 2)).unwrap();
        assert!(first.more);
        let first_products = first.rows.iter().filter(|r| r.table == "products").count();
        assert!(
            first_products >= 3,
            "the whole first transaction, though above the limit"
        );
        let rest = shop
            .db
            .read({
                let since = first.next_since;
                move |c| page(c, since, C2, PAGE_ROWS)
            })
            .unwrap();
        assert!(!rest.more);
        assert!(
            rest.rows.iter().all(|r| r.row_id != "x"),
            "routing rows are M7's"
        );
        assert!(
            rest.rows.iter().all(|r| r.table != "documents"),
            "C2's own ticket is its echo"
        );
        let for_c3 = shop
            .db
            .read(move |c| page(c, start, C3, PAGE_ROWS))
            .unwrap();
        let ticket = for_c3.rows.iter().find(|r| r.table == "documents").unwrap();
        assert_eq!(ticket.owner.as_deref(), Some(C2));
        assert!(
            ticket.image.get("_lan").is_none() && ticket.image.get("_t").is_none(),
            "markers stripped"
        );
        assert_eq!(ticket.image["series"], "C2");
    }

    #[test]
    fn acknowledging_moves_the_till_and_prunes_what_every_reader_passed() {
        let (shop, _) = host();
        rename_three(&shop);
        let head_now = shop.db.read(head).unwrap();
        let pruned = shop
            .db
            .write(move |tx| acknowledge(tx, C2, head_now, 1))
            .unwrap();
        assert_eq!(pruned, 0, "C3 has not read them yet");
        let pruned = shop
            .db
            .write(move |tx| acknowledge(tx, C3, head_now, 1))
            .unwrap();
        assert!(pruned >= 3);
        let left: i64 = shop
            .db
            .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM lan_commits", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(left, 0);
        shop.db.write(move |tx| acknowledge(tx, C2, 0, 2)).unwrap();
        let position = shop
            .db
            .read(|c| {
                crate::sync::changelog::position(c, &crate::sync::changelog::lan_consumer(C2))
            })
            .unwrap();
        assert_eq!(position, Some(head_now), "never backwards");
    }

    #[test]
    fn a_host_transaction_bigger_than_the_hard_limit_is_split() {
        let (shop, _) = host();
        let start = shop.db.read(head).unwrap();
        rename_three(&shop);
        let page = shop
            .db
            .read(move |c| page_bounded(c, start, C2, 1, 2))
            .unwrap();
        assert_eq!(
            page.rows.len(),
            1,
            "split at the limit, row order keeps keys valid"
        );
        assert!(page.more);
    }

    #[test]
    fn a_standalone_database_has_an_empty_feed() {
        let db = Db::open_memory().unwrap();
        assert_eq!(
            db.read(|c| page(c, 0, C2, PAGE_ROWS)).unwrap().rows.len(),
            0
        );
        assert!(db.write(|tx| mark_commit(tx)).unwrap().is_none());
    }
}
