//! The main PC records a till's contact (when it was seen, its address, version, outbox and
//! clock) at most every 10 s of its own clock. A main PC whose clock is set back keeps recording
//! its tills: the last contact it noted is then in its future, which is no reason to wait.

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
fn a_main_pc_whose_clock_is_set_back_keeps_recording_its_tills() {
    let dir = tempfile::tempdir().unwrap();
    let offset = Arc::new(AtomicI64::new(0));
    let shift = offset.clone();
    let clock = move || now_ms() + shift.load(Ordering::SeqCst);
    let host = testing::host_with_clock(dir.path(), Arc::new(clock.clone()));
    let till = testing::till(dir.path(), &host);
    let till_id = till.terminal.id.clone();
    let recorded = || {
        let (id, now) = (till_id.clone(), clock());
        host.db
            .read(move |c| pos_core::sync::tills::list(c, now))
            .unwrap()
            .into_iter()
            .find(|t| t.terminal_id == id)
            .unwrap()
    };
    let client = testing::client(&host, &till);
    eventually("the till is online", Duration::from_secs(5), || {
        client.status().is_ok_and(|s| s.link == LinkState::Online)
    });
    // its first contact (the copy it downloaded) is on record, at the main PC's time
    assert!(recorded().last_seen_at.is_some_and(|at| at <= clock()));

    // the main PC's clock goes two hours back; the till measures it and says so on its calls
    offset.store(-2 * 3_600_000, Ordering::SeqCst);
    let set_back = clock();
    client.sync_now();
    eventually(
        "the main PC records the till's new clock difference",
        Duration::from_secs(5),
        || {
            let till = recorded();
            till.skew_ms.is_some_and(|s| s < -SKEW_BLOCK_MS)
                && till.last_seen_at.is_some_and(|at| at >= set_back)
        },
    );
    client.stop();
}
