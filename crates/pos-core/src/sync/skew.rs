//! Clocks on a shop LAN (AUD-006, spec §4.6 "time"): a till measures the main PC's clock
//! at every call, the documents keep the till's own time, and a till too far off cannot open a
//! drawer while it can see the main PC.

use super::{SKEW_BLOCK_MS, SKEW_WARN_MS};
use crate::audit::{self, Actor};
use crate::db::{meta_get, meta_set};
use crate::error::AppError;
use rusqlite::Transaction;
use serde_json::json;

const META_WARNED: &str = "lan_skew_warned";

/// Host minus till, taking the host's reading as made halfway through the call.
pub fn measure(sent_at: i64, host_time: i64, received_at: i64) -> i64 {
    host_time - (sent_at + (received_at - sent_at) / 2)
}

pub fn check_session_open(skew_ms: Option<i64>, online: bool) -> Result<(), AppError> {
    match skew_ms {
        Some(skew) if online && skew.abs() > SKEW_BLOCK_MS => Err(AppError::rejected(
            json!({ "skewMs": skew }),
            "lan_clock_skew",
        )),
        _ => Ok(()),
    }
}

/// Journals `lan.clock_skew` when the till's clock leaves the tolerance; `true` when it did.
pub fn note(
    tx: &Transaction<'_>,
    terminal_id: &str,
    skew_ms: i64,
    _now: i64,
) -> Result<bool, AppError> {
    let warned = meta_get(tx, META_WARNED)?.as_deref() == Some("1");
    let off = skew_ms.abs() > SKEW_WARN_MS;
    match (off, warned) {
        (true, false) => {
            meta_set(tx, META_WARNED, "1")?;
            audit::append(
                tx,
                &Actor::system(Some(terminal_id.to_owned())).event(
                    "lan.clock_skew",
                    "terminal",
                    Some(terminal_id),
                    json!({ "skewMs": skew_ms }),
                ),
            )?;
            Ok(true)
        }
        (false, true) => {
            meta_set(tx, META_WARNED, "0")?;
            Ok(false)
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{NOON, shop};

    #[test]
    fn skew_is_measured_from_both_ends_of_a_call() {
        // The till sent at 1000 and got the answer at 1200; the host's clock said 7_201_100.
        assert_eq!(measure(1_000, 7_201_100, 1_200), 7_200_000);
        assert_eq!(measure(5_000, 4_000, 5_000), -1_000);
    }

    #[test]
    fn a_session_does_not_open_online_with_a_clock_off_by_more_than_15_minutes() {
        check_session_open(Some(SKEW_BLOCK_MS), true).unwrap();
        let err = check_session_open(Some(-(SKEW_BLOCK_MS + 1)), true).unwrap_err();
        assert_eq!(err.code(), "lan_clock_skew");
        assert_eq!(
            err.payload().details.unwrap()["skewMs"],
            -(SKEW_BLOCK_MS + 1)
        );
        check_session_open(Some(SKEW_BLOCK_MS * 10), false).unwrap(); // offline: nothing to compare with
        check_session_open(None, true).unwrap();
    }

    #[test]
    fn a_skew_episode_is_journaled_once() {
        let shop = shop();
        let t = shop.terminal.id.clone();
        let note_at = |skew: i64| {
            let t = t.clone();
            shop.db.write(move |tx| note(tx, &t, skew, NOON)).unwrap()
        };
        assert!(!note_at(1_000));
        assert!(note_at(SKEW_WARN_MS + 1));
        assert!(!note_at(SKEW_WARN_MS + 5), "same episode");
        assert!(!note_at(0), "back on time");
        assert!(note_at(-(SKEW_WARN_MS + 1)), "a new episode");
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'lan.clock_skew'"),
            2
        );
    }
}
