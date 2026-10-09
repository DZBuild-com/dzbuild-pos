//! Fiscal robustness of the sale path on a real database file: concurrent tills never share or
//! skip a ticket number; a process killed mid-sale leaves every sale either complete or absent
//! (spec §14 crash test); the commit stays within the HDD budget (spec §2: ≤ 80 ms).

use super::{Cart, CartLine, SaleInput, create_sale};
use crate::audit;
use crate::auth::session::build_session;
use crate::company::{PriceMode, TaxRegime};
use crate::db::{Db, DbOptions};
use crate::documents::{self, DocType};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::PaymentInput;
use crate::terminal;
use crate::testkit::shop_in;
use crate::till::TillContext;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

const CHILD_ENV: &str = "DZPOS_CRASH_CHILD_DB";

/// A file database with a shop, one product in stock and the cashier's session open; returns
/// the product id.
pub(crate) fn prepared(path: &Path) -> String {
    let db = Db::open(path, DbOptions::default()).unwrap();
    let shop = shop_in(db, TaxRegime::Reel, PriceMode::Ttc);
    let product = shop.product("Lait", Money(4_500), &shop.tva19);
    shop.stock_in(&product.id, 1_000_000);
    shop.open_till(0);
    let id = product.id.clone();
    shop.db.close().unwrap();
    id
}

pub(crate) fn cashier_context(db: &Db) -> TillContext {
    let (session, pack) = db
        .read(|c| {
            let terminal = terminal::local(c)?
                .ok_or_else(|| crate::error::AppError::Internal("no local terminal".into()))?;
            let cashier: String = c.query_row(
                "SELECT id FROM users WHERE display_name = 'Yacine'",
                [],
                |r| r.get(0),
            )?;
            Ok((
                build_session(c, &cashier, &terminal, crate::time::now_ms())?,
                crate::datapack::bundled()?,
            ))
        })
        .unwrap();
    TillContext::new(session, Arc::new(pack), crate::time::now_ms())
}

pub(crate) fn one_sale(
    db: &Db,
    ctx: &TillContext,
    product_id: &str,
) -> Result<super::ValidatedSale, crate::error::AppError> {
    let mut ctx = ctx.clone();
    ctx.now = crate::time::now_ms();
    let input = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(product_id, Qty(1_000))]),
        payments: vec![PaymentInput::cash(Money(5_000))],
    };
    db.write(move |tx| create_sale(tx, &ctx, input))
}

#[test]
fn concurrent_tills_get_every_ticket_number_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    let product = prepared(&path);
    let db = Db::open(&path, DbOptions::default()).unwrap();
    let ctx = cashier_context(&db);
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let (db, ctx, product) = (db.clone(), ctx.clone(), product.clone());
            std::thread::spawn(move || {
                (0..25)
                    .map(|_| {
                        one_sale(&db, &ctx, &product)
                            .unwrap()
                            .document
                            .chain_seq
                            .unwrap()
                    })
                    .collect::<Vec<i64>>()
            })
        })
        .collect();
    let mut seqs: Vec<i64> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    seqs.sort_unstable();
    assert_eq!(seqs, (1..=100).collect::<Vec<i64>>());
    let numbers: i64 = db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(DISTINCT number) FROM documents WHERE doc_type = 'ticket'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(numbers, 100);
    let chain = db
        .read(|c| documents::verify_chain(c, DocType::Ticket, "C1"))
        .unwrap();
    assert_eq!((chain.rows, chain.first_broken_seq), (100, None));
}

/// The child process of [`a_killed_process_leaves_whole_sales_or_nothing`]: sells until killed.
/// Without the environment variable (a normal `--ignored` run) it returns at once.
#[test]
#[ignore = "child process of a_killed_process_leaves_whole_sales_or_nothing"]
fn crash_child_sells_until_killed() {
    let Ok(path) = std::env::var(CHILD_ENV) else {
        return;
    };
    let db = Db::open(Path::new(&path), DbOptions::default()).unwrap();
    let ctx = cashier_context(&db);
    let product: String = db
        .read(|c| Ok(c.query_row("SELECT id FROM products LIMIT 1", [], |r| r.get(0))?))
        .unwrap();
    loop {
        one_sale(&db, &ctx, &product).unwrap();
    }
}

fn scalar(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

/// Spec §14: kill the process while it commits sales, reopen,
/// and every sale must be either complete (document, lines, payments, stock, cash, audit) or
/// absent, with the chains and the counters intact.
#[test]
fn a_killed_process_leaves_whole_sales_or_nothing() {
    for round in 0..3_u64 {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        prepared(&path);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "sales::robustness::crash_child_sells_until_killed",
                "--exact",
                "--ignored",
                "--nocapture",
            ])
            .env(CHILD_ENV, &path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let watcher = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        let wanted = 20 + 15 * i64::try_from(round).unwrap();
        while scalar(&watcher, "SELECT COUNT(*) FROM documents") < wanted {
            assert!(Instant::now() < deadline, "the child sold nothing in time");
            assert!(
                child.try_wait().unwrap().is_none(),
                "the child stopped by itself"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(3 + 7 * round));
        child.kill().unwrap();
        child.wait().unwrap();
        drop(watcher);

        let db = Db::open(&path, DbOptions::default()).unwrap();
        assert!(db.report().unclean_shutdown && db.report().integrity_ok);
        assert!(db.integrity_check().unwrap());
        let checks = db
            .read(|c| {
                let q = |sql: &str| -> rusqlite::Result<i64> { c.query_row(sql, [], |r| r.get(0)) };
                Ok([
                    ("tickets", q("SELECT COUNT(*) FROM documents WHERE doc_type = 'ticket' AND status = 'validated'")?),
                    ("drafts left", q("SELECT COUNT(*) FROM documents WHERE status <> 'validated'")?),
                    ("tickets without a line", q("SELECT COUNT(*) FROM documents d WHERE NOT EXISTS (SELECT 1 FROM document_lines l WHERE l.document_id = d.id)")?),
                    ("tickets without payment", q("SELECT COUNT(*) FROM documents d WHERE NOT EXISTS (SELECT 1 FROM payments p WHERE p.document_id = d.id)")?),
                    ("tickets without stock movement", q("SELECT COUNT(*) FROM documents d WHERE NOT EXISTS (SELECT 1 FROM stock_movements m WHERE m.ref_id = d.id)")?),
                    ("tickets without cash movement", q("SELECT COUNT(*) FROM documents d WHERE NOT EXISTS (SELECT 1 FROM cash_movements m WHERE m.ref_id = d.id)")?),
                    ("tickets without audit row", q("SELECT COUNT(*) FROM documents d WHERE NOT EXISTS (SELECT 1 FROM audit_log a WHERE a.action = 'sale.created' AND a.entity_id = d.id)")?),
                    ("orphan stock movements", q("SELECT COUNT(*) FROM stock_movements m WHERE m.reason = 'sale' AND NOT EXISTS (SELECT 1 FROM documents d WHERE d.id = m.ref_id)")?),
                    ("orphan cash movements", q("SELECT COUNT(*) FROM cash_movements m WHERE m.kind = 'sale' AND NOT EXISTS (SELECT 1 FROM documents d WHERE d.id = m.ref_id)")?),
                    ("orphan audit rows", q("SELECT COUNT(*) FROM audit_log a WHERE a.action = 'sale.created' AND NOT EXISTS (SELECT 1 FROM documents d WHERE d.id = a.entity_id)")?),
                    ("numbers handed out", q("SELECT COALESCE(SUM(last_number), 0) FROM counters WHERE doc_type = 'ticket'")?),
                    ("stock drift", q("SELECT COUNT(*) FROM stock_levels l WHERE l.qty <> (SELECT SUM(m.qty_delta) FROM stock_movements m WHERE m.product_id = l.product_id AND m.location_id = l.location_id)")?),
                ])
            })
            .unwrap();
        let tickets = checks[0].1;
        assert!(tickets >= wanted, "round {round}: {checks:?}");
        for (what, n) in &checks[1..10] {
            assert_eq!(*n, 0, "round {round}: {what}");
        }
        assert_eq!(
            checks[10].1, tickets,
            "round {round}: a number was lost or skipped"
        );
        assert_eq!(
            checks[11].1, 0,
            "round {round}: stock levels drifted from the ledger"
        );
        let chain = db
            .read(|c| documents::verify_chain(c, DocType::Ticket, "C1"))
            .unwrap();
        assert_eq!(chain.first_broken_seq, None, "round {round}");
        let journal = db.read(audit::verify_chain).unwrap();
        assert_eq!(journal.first_broken_seq, None, "round {round}");
        // The till goes on selling after the restart.
        let ctx = cashier_context(&db);
        let product: String = db
            .read(|c| Ok(c.query_row("SELECT id FROM products LIMIT 1", [], |r| r.get(0))?))
            .unwrap();
        let next = one_sale(&db, &ctx, &product).unwrap();
        assert_eq!(next.document.chain_seq, Some(tickets + 1));
    }
}

/// Spec §2 budget: sale commit ≤ 80 ms on the reference HDD. CI asserts a loose 200 ms p95 and
/// prints the real figures (`--nocapture`); re-run on the Celeron J1900 in the M1 smoke check.
#[test]
fn sale_commit_p95_stays_within_budget() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    let product = prepared(&path);
    let db = Db::open(&path, DbOptions::default()).unwrap();
    let ctx = cashier_context(&db);
    one_sale(&db, &ctx, &product).unwrap();
    let mut times: Vec<Duration> = (0..200)
        .map(|_| {
            let start = Instant::now();
            one_sale(&db, &ctx, &product).unwrap();
            start.elapsed()
        })
        .collect();
    times.sort_unstable();
    let (p50, p95, max) = (times[100], times[189], times[199]);
    println!(
        "sale commit on a file database: p50 {p50:?}, p95 {p95:?}, max {max:?} (budget 80 ms on HDD)"
    );
    assert!(p95 < Duration::from_millis(200), "p95 {p95:?}");
}
