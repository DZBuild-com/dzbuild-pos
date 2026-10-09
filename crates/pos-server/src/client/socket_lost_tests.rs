//! With the main PC's WebSocket open, the engine waits 30 s between two cycles (the socket pokes
//! it on every change). When that socket closes, the main PC may be gone: the engine runs a cycle
//! at once, so the till says "offline" within seconds instead of at the next poll.

use crate::testing;
use pos_core::sync::status::LinkState;
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

fn products_named(till: &testing::Till, name: &'static str) -> i64 {
    till.db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM products WHERE name_fr = ?1",
                [name],
                |r| r.get(0),
            )?)
        })
        .unwrap()
}

#[test]
fn a_till_hears_at_once_that_its_main_pc_went_away() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = testing::host(dir.path());
    let till = testing::till(dir.path(), &host);
    let client = testing::client(&host, &till);
    eventually(
        "the till is online with the main PC's socket",
        Duration::from_secs(5),
        || client.is_online() && client.has_socket(),
    );
    // a change on the main PC: the socket's poke brings it, then the engine waits the 30 s of a
    // live socket
    host.product("Pain", 1_000);
    eventually("the poke brings the change", Duration::from_secs(5), || {
        products_named(&till, "Pain") == 1
    });
    std::thread::sleep(Duration::from_millis(500));
    assert!(client.is_online());

    // the main PC goes away
    if let Some(server) = host.server.take() {
        server.stop();
    }
    eventually(
        "the till says it is offline",
        Duration::from_secs(5),
        || client.status().is_ok_and(|s| s.link == LinkState::Offline),
    );
    client.stop();
}
