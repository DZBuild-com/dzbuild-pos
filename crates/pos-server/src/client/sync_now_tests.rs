//! "Sync now" is a whole cycle: the main PC is checked again (identity, clock, schema) at once,
//! not at the next 30 s check. The owner who corrected a PC's clock, or updated the main PC,
//! sees it measured again when they ask; the pokes of the outbox hook and of the WebSocket still
//! only replay and pull.

use crate::testing;
use pos_core::sync::SKEW_BLOCK_MS;
use pos_core::sync::status::LinkState;
use pos_core::time::now_ms;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

fn eventually(what: &str, within: Duration, check: impl Fn() -> bool) {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if check() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("{what} did not happen within {within:?}");
}

#[test]
fn sync_now_measures_the_main_pcs_clock_again() {
    let dir = tempfile::tempdir().unwrap();
    // a main PC whose clock the test can set back
    let offset = Arc::new(AtomicI64::new(0));
    let shift = offset.clone();
    let host = testing::host_with_clock(
        dir.path(),
        Arc::new(move || now_ms() + shift.load(Ordering::SeqCst)),
    );
    let till = testing::till(dir.path(), &host);
    let client = testing::client(&host, &till);
    eventually("the till is online", Duration::from_secs(5), || {
        client.status().is_ok_and(|s| s.link == LinkState::Online)
    });
    assert!(client.skew_ms().is_some_and(|s| s.abs() < 5_000));

    // the main PC's clock goes two hours back; the till's owner asks to sync
    offset.store(-2 * 3_600_000, Ordering::SeqCst);
    client.sync_now();
    eventually(
        "the till measures the difference again",
        Duration::from_secs(5),
        || client.skew_ms().is_some_and(|s| s < -SKEW_BLOCK_MS),
    );
    client.stop();
}
