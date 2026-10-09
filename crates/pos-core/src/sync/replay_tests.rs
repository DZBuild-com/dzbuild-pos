//! Ownership of a replayed update (M5 Task 9 review). The till's guard refuses an update of a row
//! that is not the till's before (OLD) or after (NEW) the write; the main PC checks the same two
//! images, so a till cannot rewrite one of its rows into another terminal's (an open session, a
//! draft ticket). The module's other tests are in `replay.rs`.

use super::outbox::{self, ItemResult, ItemStatus, WireItem};
use super::replay::{Received, apply_batch};
use super::testkit::{LanPair, lan_pair};
use crate::cash::closing::{CloseInput, close_session};
use crate::testkit::NOON;
use serde_json::json;

/// What the main PC holds about the sessions and the till's items: the till's open sessions,
/// the main PC's own sessions, the closures, the inbox, the till's journal rows on the main PC and
/// the till's last replayed seq.
type HostState = (i64, i64, i64, i64, i64, Option<i64>);

fn host_state(lan: &LanPair) -> HostState {
    let (till, c1) = (lan.till_terminal.id.clone(), lan.host.terminal.id.clone());
    lan.host
        .db
        .read(move |c| {
            let count = |sql: &str, id: &str| -> rusqlite::Result<i64> {
                c.query_row(sql, [id], |r| r.get(0))
            };
            Ok((
                count(
                    "SELECT COUNT(*) FROM cash_sessions WHERE terminal_id = ?1 AND status = 'open'",
                    &till,
                )?,
                count(
                    "SELECT COUNT(*) FROM cash_sessions WHERE terminal_id = ?1",
                    &c1,
                )?,
                c.query_row("SELECT COUNT(*) FROM cash_closures", [], |r| r.get(0))?,
                c.query_row("SELECT COUNT(*) FROM lan_inbox", [], |r| r.get(0))?,
                c.query_row("SELECT COUNT(*) FROM lan_journal", [], |r| r.get(0))?,
                c.query_row(
                    "SELECT last_replayed_seq FROM lan_devices WHERE terminal_id = ?1",
                    [&till],
                    |r| r.get(0),
                )?,
            ))
        })
        .unwrap()
}

/// A session opened on the till and recorded on the main PC, then closed on the till: the close
/// item, not sent yet.
fn a_close_to_send(lan: &LanPair) -> WireItem {
    lan.open_till_session(0);
    let opened = lan.push();
    assert!(
        opened.iter().all(|r| r.status == ItemStatus::Applied),
        "{opened:?}"
    );
    let ctx = lan.till_ctx();
    lan.till
        .write(move |tx| close_session(tx, &ctx, CloseInput::default()))
        .unwrap();
    let mut pending = lan.till.read(|c| outbox::pending(c, 10)).unwrap();
    assert_eq!(pending.len(), 1, "the close");
    pending.remove(0)
}

/// The item's `cash_sessions` image now names the main PC's terminal.
fn moved_to_the_main_pc(lan: &LanPair, mut item: WireItem) -> WireItem {
    let c1 = lan.host.terminal.id.clone();
    let mut moved = 0;
    for row in &mut item.bundle.rows {
        if row.table == "cash_sessions" {
            row.image.insert("terminal_id".into(), json!(c1));
            moved += 1;
        }
    }
    assert_eq!(moved, 1, "the close updates the session once");
    item
}

fn send(lan: &LanPair, item: WireItem) -> Vec<ItemResult> {
    apply_batch(
        &lan.host.db,
        &lan.sender(),
        vec![item],
        &Received {
            at: NOON + 2_000,
            skew_ms: Some(0),
        },
    )
    .unwrap()
}

fn assert_refused_and_nothing_changed(lan: &LanPair, results: &[ItemResult], before: HostState) {
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].status, ItemStatus::Rejected);
    assert_eq!(results[0].code.as_deref(), Some("replay_forbidden_row"));
    assert_eq!(
        host_state(lan),
        before,
        "nothing changed: the session stays the till's and open, no Z, no inbox row"
    );
    assert_eq!(
        lan.count_host(
            "SELECT COUNT(*) FROM lan_notices WHERE kind = 'replay_rejected' AND acknowledged_at IS NULL"
        ),
        1
    );
}

#[test]
fn a_till_cannot_close_its_session_as_another_terminals() {
    let lan = lan_pair();
    let genuine = a_close_to_send(&lan);
    let forged = moved_to_the_main_pc(&lan, genuine);
    let before = host_state(&lan);
    assert_eq!(
        (before.0, before.1, before.2, before.3),
        (1, 0, 0, 1),
        "the till's session is open on the main PC"
    );
    let results = send(&lan, forged);
    assert_refused_and_nothing_changed(&lan, &results, before);
    // The till's own close, as it was made, is still recorded.
    let results = lan.push();
    assert!(
        results.iter().all(|r| r.status == ItemStatus::Applied),
        "{results:?}"
    );
    let after = host_state(&lan);
    assert_eq!(
        (after.0, after.1, after.2, after.3),
        (0, 0, 1, 2),
        "closed as the till's, with its Z"
    );
}

/// The same update alone: no trigger of the main PC stops it (the session is still open there,
/// and no closure checks it), so only the ownership of the image written refuses it.
#[test]
fn a_till_cannot_hand_its_open_session_to_another_terminal() {
    let lan = lan_pair();
    let genuine = a_close_to_send(&lan);
    let mut forged = moved_to_the_main_pc(&lan, genuine);
    forged.bundle.rows.retain(|r| r.table == "cash_sessions");
    forged.bundle.audit.clear();
    let before = host_state(&lan);
    let results = send(&lan, forged);
    assert_refused_and_nothing_changed(&lan, &results, before);
}
