//! The optional DZBuild link (spec §4.7, milestone M7). Nothing outside this module depends on
//! it: a shop that never links runs exactly as before. Tokens never reach SQLite or the logs.

pub mod api;
pub mod config;
pub mod derive;
pub mod engine;
pub mod http;
pub mod link;
pub mod loopback;
pub mod oauth;
pub mod orders;
pub mod outbox;
pub mod payloads;
pub mod publish;
pub mod pull;
pub mod push;
pub mod secrets;
pub mod stock;
pub mod store;
pub mod tokens;
pub mod variants;
pub mod wire;

#[cfg(test)]
mod orders_extras_tests;
#[cfg(test)]
mod refused_tests;
#[cfg(test)]
pub(crate) mod testkit;
#[cfg(test)]
mod unreadable_tests;

pub use config::CloudConfig;
pub use engine::{SyncHandle, SyncStatus};
pub use http::{Call, CallError, Clock, Endpoint, Http, Lane, SystemClock};

/// An `Idempotency-Key` (1–64 characters of `[A-Za-z0-9_:.-]`): `<kind>:<local id>` when it
/// fits, else `<kind>:<first 32 hex of SHA-256(local id)>`. Deterministic, so a retry reuses it.
pub fn outbox_key(kind: &str, local: &str) -> String {
    let safe = |s: &str| {
        s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_:.-".contains(&b))
    };
    let plain = format!("{kind}:{local}");
    if plain.len() <= 64 && safe(&plain) {
        return plain;
    }
    use sha2::{Digest, Sha256};
    let digest = hex::encode(Sha256::digest(local.as_bytes()).as_slice());
    format!("{kind}:{}", &digest[..32])
}

#[cfg(test)]
mod key_tests {
    #[test]
    fn keys_are_short_safe_and_deterministic() {
        let id = "00000000-0000-7000-8000-00000000f001";
        assert_eq!(super::outbox_key("sale", id), format!("sale:{id}"));
        let long = super::outbox_key("device", &"x".repeat(80));
        assert_eq!(long.len(), 7 + 32);
        assert_eq!(long, super::outbox_key("device", &"x".repeat(80)));
        let odd = super::outbox_key("media", "p1/photo é.jpg");
        assert!(
            odd.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_:.-".contains(&b))
        );
    }
}
