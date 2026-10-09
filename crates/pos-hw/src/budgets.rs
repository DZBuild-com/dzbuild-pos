//! NFR-007: an Arabic receipt reaches the printer within 1.5 s; the software share (layout,
//! shaping, raster, ESC/POS bytes) must stay under 300 ms on the reference PC, leaving the rest
//! to the USB/spooler transfer and the print head. Run by `scripts/perf/bench.ps1` in release.

use crate::profile::{PaperWidth, Preset, Profile};
use crate::raster::Typesetter;
use crate::receipt::receipt_layout;
use crate::render::{DrawerKick, Job, render};
use pos_core::lang::Lang;
use std::time::{Duration, Instant};

#[test]
#[ignore = "budget: cargo test --release -p pos-hw budget_ -- --ignored --nocapture"]
fn budget_arabic_receipt_render_under_300_ms() {
    let mut receipt = crate::testdata::ticket(Lang::Ar);
    let first = receipt.lines[0].clone();
    while receipt.lines.len() < 30 {
        receipt.lines.push(first.clone());
    }
    let layout = receipt_layout(&receipt, Some(Lang::Fr));
    let profile = Profile::preset(Preset::Xprinter, PaperWidth::Mm80);
    let mut times: Vec<Duration> = (0..5)
        .map(|_| {
            let start = Instant::now();
            let job = Job {
                layout: &layout,
                logo: None,
                drawer: DrawerKick::AfterPrint,
            };
            let out = render(&job, &profile, &mut Typesetter::new()).unwrap();
            assert!(out.raster && !out.bytes.is_empty());
            start.elapsed()
        })
        .collect();
    times.sort_unstable();
    let median = times[2];
    println!("{}", render_line(median));
    assert!(median <= Duration::from_millis(300), "{median:?}");
}

/// The `BUDGET arabic_receipt_render <ms> ms budget 300 ms <OK|OVER>` line. It starts with a line
/// break: libtest has already written `test <name> ... ` on the current line.
fn render_line(median: Duration) -> String {
    let verdict = if median <= Duration::from_millis(300) {
        "OK"
    } else {
        "OVER"
    };
    format!(
        "\nBUDGET arabic_receipt_render {} ms budget 300 ms {verdict}",
        median.as_millis()
    )
}

/// Under `--nocapture --test-threads=1` libtest writes `test <name> ... ` with no line break
/// before the test runs and `ok` after it; `bench.ps1` keeps only the lines that match
/// `^BUDGET `, so the line must begin its own line.
#[test]
fn the_render_line_begins_its_own_line_after_the_libtest_prefix() {
    let output = format!(
        "running 1 test\ntest budgets::budget_arabic_receipt_render_under_300_ms ... {}\nok\n",
        render_line(Duration::from_millis(79))
    );
    let collected: Vec<&str> = output
        .lines()
        .filter(|line| line.starts_with("BUDGET "))
        .collect();
    assert_eq!(
        collected,
        vec!["BUDGET arabic_receipt_render 79 ms budget 300 ms OK"]
    );
}
