//! DZBuild POS licences (M9): the code a customer sends to DZBuild to get a licence for this PC
//! (`device`), the licence key (`DZL1.<payload>.<signature>`, Ed25519) verified offline against the
//! public keys built into the app, and the licence kept three times on this PC (`app_meta`, the
//! Windows Credential Manager, a file under `%LOCALAPPDATA%`). No Tauri type: the shell wraps it
//! (`src-tauri/src/license.rs`). What a desktop app can and cannot do against tampering is written
//! in `docs/release/licensing.md` § Limits.

pub mod codes;
pub mod device;
#[cfg(test)]
mod device_tests;
pub mod engine;
#[cfg(test)]
mod engine_tests;
pub mod key;
pub mod keys;
pub mod payload;
pub mod record;
pub mod state;
pub mod store;
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
pub mod verify;

/// Crate name, checked by the smoke test.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod smoke {
    #[test]
    fn crate_is_named() {
        assert_eq!(super::CRATE, "pos-license");
    }
}
