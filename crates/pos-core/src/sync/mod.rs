//! LAN (M5) and cloud (M7) sync. `catalogue` says what replicates and who owns each row;
//! the other modules build the change feed, the till's outbox and the host's replay on it.

pub mod answers;
pub mod apply;
pub mod capture;
#[cfg(test)]
mod capture_tests;
pub mod catalogue;
#[cfg(test)]
mod catalogue_tests;
pub mod changelog;
pub mod devices;
pub mod feed;
pub mod journal;
pub mod notices;
pub mod outbox;
pub mod pairing;
#[cfg(test)]
mod promotion_tests;
pub mod replay;
#[cfg(test)]
mod replay_refusal_tests;
#[cfg(test)]
mod replay_skew_tests;
#[cfg(test)]
mod replay_tests;
pub mod replica;
pub mod role;
pub mod skew;
pub mod snapshot;
pub mod status;
#[cfg(test)]
pub(crate) mod testkit;
pub mod tills;

/// A till's clock off by more than this raises a notice and a warning (AUD-006).
pub const SKEW_WARN_MS: i64 = 5 * 60_000;
/// … and by more than this, the till cannot open a cash session while connected.
pub const SKEW_BLOCK_MS: i64 = 15 * 60_000;

/// Handle to the background sync tasks held by `AppState` (spec §4.2). The LAN runtime lives in
/// `src-tauri` (it needs `pos-server`); this stays the cloud engine's slot for M7.
#[derive(Debug, Clone, Default)]
pub struct Handle {}
