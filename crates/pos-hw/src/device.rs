//! What the device threads report (scale, pole display, serial scanner) and how they retry.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Scale,
    Vfd,
    SerialScanner,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "state",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum DeviceState {
    /// Not configured on this PC.
    Off,
    Connected {
        port: String,
    },
    /// `code` is an `errors:` key (`port_unavailable`, `scale_not_responding`…).
    Disconnected {
        port: String,
        code: String,
    },
}

pub type StatusSink = Arc<dyn Fn(DeviceKind, &DeviceState) + Send + Sync>;

pub fn silent_status() -> StatusSink {
    Arc::new(|_, _| {})
}

/// 0.5 s, 1 s, 2 s, 4 s, then 5 s between reconnection attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    next: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Backoff::new()
    }
}

impl Backoff {
    pub const FIRST: Duration = Duration::from_millis(500);
    pub const LONGEST: Duration = Duration::from_secs(5);

    pub fn new() -> Backoff {
        Backoff {
            next: Backoff::FIRST,
        }
    }

    pub fn fail(&mut self) -> Duration {
        let wait = self.next;
        self.next = (self.next * 2).min(Backoff::LONGEST);
        wait
    }

    pub fn reset(&mut self) {
        self.next = Backoff::FIRST;
    }
}

/// Sleeps `total` in 20 ms steps, returning early once `stop` is set.
pub fn sleep_unless(stop: &AtomicBool, total: Duration) {
    let step = Duration::from_millis(20);
    let mut slept = Duration::ZERO;
    while slept < total && !stop.load(Ordering::SeqCst) {
        std::thread::sleep(step.min(total - slept));
        slept += step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_up_to_five_seconds_and_resets() {
        let mut b = Backoff::new();
        let waits: Vec<u64> = (0..6).map(|_| b.fail().as_millis() as u64).collect();
        assert_eq!(waits, [500, 1_000, 2_000, 4_000, 5_000, 5_000]);
        b.reset();
        assert_eq!(b.fail().as_millis(), 500);
    }

    #[test]
    fn states_cross_the_ipc_tagged() {
        let json = serde_json::to_value(DeviceState::Disconnected {
            port: "COM3".into(),
            code: "port_unavailable".into(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "state": "disconnected", "port": "COM3", "code": "port_unavailable" })
        );
        assert_eq!(
            serde_json::to_value(DeviceKind::SerialScanner).unwrap(),
            "serial_scanner"
        );
    }
}
