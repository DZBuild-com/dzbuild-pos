//! DZBuild POS LAN (spec §3, §4.6): the main PC's HTTP + WebSocket server (`host`), the till's
//! engine (`client`), mDNS discovery and the Windows firewall check. The rules of the LAN live
//! in `pos_core::sync`; this crate only carries them over the network.

pub mod client;
pub mod discovery;
pub mod firewall;
pub mod host;
pub mod wire;

#[cfg(test)]
pub(crate) mod testing;

/// Crate name, checked by the workspace smoke test.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod smoke {
    #[test]
    fn crate_is_named() {
        assert_eq!(super::CRATE, "pos-server");
    }
}
