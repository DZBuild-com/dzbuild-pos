//! The reference budgets (spec §2, NFR-003…009) measured. Every test is ignored: a debug build
//! or a busy CI runner tells nothing about a Celeron J1900 with an HDD. `scripts/perf/bench.ps1`
//! runs them in release on the reference PC and collects the lines that match `^BUDGET `:
//! `cargo test --release -p pos-core budget_ -- --ignored --nocapture --test-threads=1`.

use crate::catalog::UNIT_PIECE_ID;
use crate::catalog::normalize;
use crate::catalog::search::{SearchQuery, find_by_code, search};
use crate::company::{PriceMode, TaxRegime};
use crate::db::{Db, DbOptions};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::PaymentInput;
use crate::sales::robustness::{cashier_context, one_sale, prepared};
use crate::sales::{Cart, CartLine, SaleInput, create_sale};
use crate::testkit::shop_in;
use rusqlite::params;
use std::path::Path;
use std::time::{Duration, Instant};

/// NFR-009's five busy years.
const FIVE_YEARS_BUDGET_BYTES: u64 = 2 * 1_073_741_824;

/// One `BUDGET <name> <measured> ms budget <limit> ms <OK|OVER>` line. It starts with a line
/// break: under `--nocapture --test-threads=1` libtest has already written `test <name> ... `
/// on the current line, and `bench.ps1` keeps only the lines that match `^BUDGET `.
fn budget_line(name: &str, measured: Duration, budget: Duration) -> String {
    let verdict = if measured <= budget { "OK" } else { "OVER" };
    format!(
        "\nBUDGET {name} {} ms budget {} ms {verdict}",
        measured.as_micros() as f64 / 1_000.0,
        budget.as_millis()
    )
}

/// The busy year's line (its size, five times that, the verdict against 2 GiB), starting with a
/// line break like `budget_line`.
fn busy_year_line(bytes: u64) -> String {
    let five_years = bytes * 5;
    let verdict = if five_years < FIVE_YEARS_BUDGET_BYTES {
        "OK"
    } else {
        "OVER"
    };
    format!(
        "\nBUDGET busy_year_size {} MiB five_years {} MiB budget 2048 MiB {verdict}",
        bytes / 1_048_576,
        five_years / 1_048_576
    )
}

fn report(name: &str, measured: Duration, budget: Duration) {
    println!("{}", budget_line(name, measured, budget));
}

fn p95(mut times: Vec<Duration>) -> Duration {
    times.sort_unstable();
    let index = (times.len() * 95 / 100).min(times.len().saturating_sub(1));
    times.get(index).copied().unwrap_or_default()
}

/// `count` products with an internal barcode each, inserted directly (a 50 000-line import
/// would take minutes through the service and measure the import, not the lookup).
fn seed_products(db: &Db, count: usize) -> Vec<String> {
    let tva: String = db
        .read(|c| {
            Ok(
                c.query_row("SELECT id FROM tax_rates WHERE code = 'TVA19'", [], |r| {
                    r.get(0)
                })?,
            )
        })
        .unwrap();
    db.write(move |tx| {
        let mut product = tx.prepare(
            "INSERT INTO products (id, reference, name_ar, name_fr, search_text, unit_id, tax_rate_id,
                                   sell_price, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 10000, 0, 0)",
        )?;
        let mut barcode = tx.prepare(
            "INSERT INTO product_barcodes (id, product_id, barcode, pack_qty, created_at) VALUES (?1, ?2, ?3, 1000, 0)",
        )?;
        let ar = ["حليب", "قهوة", "سكر", "زيت", "خبز", "جبن", "ماء", "شاي", "أرز", "عصير"];
        let fr = ["Lait", "Café", "Sucre", "Huile", "Pain", "Fromage", "Eau", "Thé", "Riz", "Jus"];
        let brands = ["Candia", "Soummam", "Cevital", "Hamoud", "Ifri", "Elio", "Safia", "Ramdy", "Danone", "Tchina"];
        let mut codes = Vec::with_capacity(count);
        for i in 0..count {
            let id = crate::ids::new_id();
            let name_ar = format!("ال{} {} {i}", ar[i % 10], brands[i / 10 % 10]);
            let name_fr = format!("{} {} {}g", fr[i % 10], brands[i / 10 % 10], i % 997);
            let reference = format!("REF-{i:06}");
            let code = format!("201{i:010}");
            let text = normalize::search_text([name_ar.as_str(), name_fr.as_str(), reference.as_str(), code.as_str()]);
            product.execute(params![id, reference, name_ar, name_fr, text, UNIT_PIECE_ID, tva])?;
            barcode.execute(params![crate::ids::new_id(), id, code])?;
            codes.push(code);
        }
        Ok(codes)
    })
    .unwrap()
}

/// NFR-003: SQLite open < 50 ms, on a closed file database of 50 000 products (median of 5).
#[test]
#[ignore = "budget: cargo test --release -p pos-core budget_ -- --ignored --nocapture"]
fn budget_db_open_under_50_ms() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    let shop = shop_in(
        Db::open(&path, DbOptions::default()).unwrap(),
        TaxRegime::Reel,
        PriceMode::Ttc,
    );
    seed_products(&shop.db, 50_000);
    shop.db.close().unwrap(); // the only handle: the file is closed and checkpointed
    let mut times: Vec<Duration> = (0..5)
        .map(|_| {
            let start = Instant::now();
            let db = Db::open(&path, DbOptions::default()).unwrap();
            let elapsed = start.elapsed();
            db.close().unwrap();
            elapsed
        })
        .collect();
    times.sort_unstable();
    let median = times[2];
    report("db_open", median, Duration::from_millis(50));
    assert!(median < Duration::from_millis(50), "{median:?}");
}

/// NFR-005 (Rust share): a barcode lookup on 50 000 products, p95 of 500 scans ≤ 20 ms, which
/// leaves 30 ms of the 50 ms scan → line budget to the webview (measured by the E2E suite).
#[test]
#[ignore = "budget: cargo test --release -p pos-core budget_ -- --ignored --nocapture"]
fn budget_scan_lookup_p95_under_20_ms() {
    let dir = tempfile::tempdir().unwrap();
    let shop = shop_in(
        Db::open(&dir.path().join("pos.db"), DbOptions::default()).unwrap(),
        TaxRegime::Reel,
        PriceMode::Ttc,
    );
    let codes = seed_products(&shop.db, 50_000);
    let picks: Vec<String> = (0..500)
        .map(|i| codes[(i * 7_919) % codes.len()].clone())
        .collect();
    let times = shop
        .db
        .read(move |c| {
            find_by_code(c, &picks[0])?; // warm the statement cache
            let mut times = Vec::with_capacity(picks.len());
            for code in &picks {
                let start = Instant::now();
                let found = find_by_code(c, code)?;
                times.push(start.elapsed());
                assert!(found.is_some(), "{code}");
            }
            Ok(times)
        })
        .unwrap();
    let p = p95(times);
    report("scan_lookup_p95", p, Duration::from_millis(20));
    assert!(p <= Duration::from_millis(20), "{p:?}");
}

/// NFR-006: sale commit including fsync (`synchronous=FULL`) ≤ 80 ms at p95 on the HDD; M1a's
/// `sale_commit_p95_stays_within_budget` keeps a loose 200 ms in every run.
#[test]
#[ignore = "budget: cargo test --release -p pos-core budget_ -- --ignored --nocapture"]
fn budget_sale_commit_p95_under_80_ms() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    let product = prepared(&path);
    let db = Db::open(&path, DbOptions::default()).unwrap();
    let ctx = cashier_context(&db);
    one_sale(&db, &ctx, &product).unwrap();
    let times: Vec<Duration> = (0..300)
        .map(|_| {
            let start = Instant::now();
            one_sale(&db, &ctx, &product).unwrap();
            start.elapsed()
        })
        .collect();
    let p = p95(times);
    report("sale_commit_p95", p, Duration::from_millis(80));
    assert!(p <= Duration::from_millis(80), "{p:?}");
}

/// NFR-008: < 100 ms end to end on 100 000 products: the search and its JSON (what crosses the
/// IPC), per query, averaged over 10 runs.
#[test]
#[ignore = "budget: cargo test --release -p pos-core budget_ -- --ignored --nocapture"]
fn budget_search_100k_end_to_end_under_100_ms() {
    let dir = tempfile::tempdir().unwrap();
    let shop = shop_in(
        Db::open(&dir.path().join("pos.db"), DbOptions::default()).unwrap(),
        TaxRegime::Reel,
        PriceMode::Ttc,
    );
    seed_products(&shop.db, 100_000);
    let worst = shop
        .db
        .read(|c| {
            let mut worst = Duration::ZERO;
            for text in ["lait", "lait cand", "حليب", "cafe so", "REF-099999", "12"] {
                let query = SearchQuery {
                    text: text.into(),
                    ..SearchQuery::default()
                };
                search(c, &query)?;
                let start = Instant::now();
                for _ in 0..10 {
                    let hits = search(c, &query)?;
                    let json = serde_json::to_string(&hits)?;
                    assert!(!json.is_empty());
                }
                worst = worst.max(start.elapsed() / 10);
            }
            Ok(worst)
        })
        .unwrap();
    report("search_100k_end_to_end", worst, Duration::from_millis(100));
    assert!(worst < Duration::from_millis(100), "{worst:?}");
}

/// NFR-009: a busy shop's year — 300 tickets of 4 lines a day, one Z a day — through the real
/// sale service, one transaction per day (the size, not the fsync, is measured here); five years
/// must stay under ~2 GB.
#[test]
#[ignore = "budget: minutes in release; cargo test --release -p pos-core budget_busy_year_file_size -- --ignored --nocapture"]
fn budget_busy_year_file_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    let shop = shop_in(
        Db::open(&path, DbOptions::default()).unwrap(),
        TaxRegime::Reel,
        PriceMode::Ttc,
    );
    let products: Vec<String> = (0..200)
        .map(|i| {
            let p = shop.product(&format!("Article {i}"), Money(5_000 + i * 150), &shop.tva19);
            shop.stock_in(&p.id, 100_000_000);
            p.id
        })
        .collect();
    let day = 86_400_000_i64;
    let start = crate::testkit::NOON - 365 * day;
    for d in 0..365_i64 {
        let when = start + d * day;
        let mut ctx = shop.till();
        ctx.now = when;
        let open = ctx.clone();
        shop.db
            .write(move |tx| crate::cash::open_session(tx, &open, Money(500_000)).map(drop))
            .unwrap();
        let items = products.clone();
        let batch = ctx.clone();
        shop.db
            .write(move |tx| {
                for t in 0..300_usize {
                    let lines = (0..4)
                        .map(|l| CartLine::product(&items[(t * 4 + l) % items.len()], Qty(1_000)))
                        .collect();
                    let input = SaleInput {
                        cart: Cart::new(Lang::Fr, lines),
                        payments: vec![PaymentInput::cash(Money(1_000_000))],
                    };
                    create_sale(tx, &batch, input)?;
                }
                Ok(())
            })
            .unwrap();
        let close = ctx.clone();
        shop.db
            .write(move |tx| {
                let session = crate::cash::require_open(tx, &close.session.terminal_id)?;
                let expected = crate::cash::expected_cash(tx, &session.id)?;
                crate::cash::close_session(
                    tx,
                    &close,
                    crate::cash::CloseInput {
                        counts: vec![crate::cash::CountLine {
                            denomination: expected,
                            count: 1,
                        }],
                        notes: None,
                    },
                )
                .map(drop)
            })
            .unwrap();
    }
    shop.db.checkpoint().unwrap();
    let bytes = file_size(&path);
    let five_years = bytes * 5;
    println!("{}", busy_year_line(bytes));
    assert!(five_years < FIVE_YEARS_BUDGET_BYTES, "{five_years} bytes");
}

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
        + std::fs::metadata(path.with_extension("db-wal"))
            .map(|m| m.len())
            .unwrap_or(0)
}

/// The lines `bench.ps1` keeps from the test output: `Where-Object { $_ -match "^BUDGET " }`.
fn collected(output: &str) -> Vec<&str> {
    output
        .lines()
        .filter(|line| line.starts_with("BUDGET "))
        .collect()
}

/// Under `--nocapture --test-threads=1` libtest writes `test <name> ... ` with no line break
/// before the test runs and `ok` after it; every BUDGET line must still begin its own line.
#[test]
fn every_budget_line_begins_its_own_line_after_the_libtest_prefix() {
    let output = format!(
        "running 2 tests\ntest budgets::budget_db_open_under_50_ms ... {}\nok\n\
         test budgets::budget_busy_year_file_size ... {}\nok\n",
        budget_line(
            "db_open",
            Duration::from_micros(26_513),
            Duration::from_millis(50)
        ),
        busy_year_line(787 * 1_048_576),
    );
    assert_eq!(
        collected(&output),
        vec![
            "BUDGET db_open 26.513 ms budget 50 ms OK",
            "BUDGET busy_year_size 787 MiB five_years 3935 MiB budget 2048 MiB OVER",
        ]
    );
}
