//! The till's side (spec §4.6): joining a main PC, then an engine that sends the outbox and
//! mirrors the main PC's feed in the background. Selling never waits for it.

pub mod api;
pub mod engine;
pub mod join;

#[cfg(test)]
mod rejoin_tests;
#[cfg(test)]
mod socket_lost_tests;
#[cfg(test)]
mod sync_now_tests;

pub use engine::{ClientConfig, ClientEvents, LanClient, RpcReply};
