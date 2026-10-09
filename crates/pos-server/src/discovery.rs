//! mDNS discovery of the main PC (LAN-004, research 07 §6.2): the main PC advertises
//! `_dzpos._tcp` with its shop name and host id; a new till lists what answers within a few
//! seconds. Typing the address stays possible (mDNS blocked by a router or a firewall).

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use pos_core::error::AppError;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub const SERVICE_TYPE: &str = "_dzpos._tcp.local.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredHost {
    pub host_id: String,
    pub name: String,
    pub addresses: Vec<String>,
    pub port: u16,
    pub app_version: Option<String>,
    pub schema_version: Option<i64>,
}

fn failed(error: impl std::fmt::Display) -> AppError {
    tracing::warn!(%error, "mDNS");
    AppError::conflict("lan_discovery_failed")
}

/// The shop's name (without dots or backslashes) and the first 8 characters of the host id,
/// within the 63 bytes of a DNS label.
pub fn instance_name(shop_name: &str, host_id: &str) -> String {
    let short: String = host_id.chars().take(8).collect();
    let mut name: String = shop_name
        .chars()
        .filter(|c| !matches!(c, '.' | '\\') && !c.is_control())
        .collect();
    let budget = 63 - short.len() - 1;
    while name.len() > budget {
        name.pop();
    }
    format!("{} {short}", name.trim())
}

pub struct Advertiser {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Advertiser {
    pub fn start(
        host_id: &str,
        shop_name: &str,
        port: u16,
        app_version: &str,
        schema_version: i64,
    ) -> Result<Advertiser, AppError> {
        let daemon = ServiceDaemon::new().map_err(failed)?;
        let host_name = format!(
            "dzpos-{}.local.",
            host_id.chars().take(8).collect::<String>()
        );
        let schema = schema_version.to_string();
        let properties = [
            ("id", host_id),
            ("name", shop_name),
            ("v", app_version),
            ("s", schema.as_str()),
        ];
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name(shop_name, host_id),
            &host_name,
            "",
            port,
            &properties[..],
        )
        .map_err(failed)?
        .enable_addr_auto();
        let fullname = info.get_fullname().to_owned();
        daemon.register(info).map_err(failed)?;
        Ok(Advertiser { daemon, fullname })
    }

    pub fn stop(self) {
        if let Err(error) = self.daemon.unregister(&self.fullname) {
            tracing::warn!(%error, "mDNS unregister");
        }
        if let Err(error) = self.daemon.shutdown() {
            tracing::warn!(%error, "mDNS shutdown");
        }
    }
}

/// Every main PC answering within `timeout`, one entry per host id.
pub fn browse(timeout: Duration) -> Result<Vec<DiscoveredHost>, AppError> {
    let daemon = ServiceDaemon::new().map_err(failed)?;
    let receiver = daemon.browse(SERVICE_TYPE).map_err(failed)?;
    let deadline = Instant::now() + timeout;
    let mut found: BTreeMap<String, DiscoveredHost> = BTreeMap::new();
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match receiver.recv_timeout(left) {
            Ok(ServiceEvent::ServiceResolved(service)) => {
                let Some(host_id) = service.get_property_val_str("id").map(str::to_owned) else {
                    continue;
                };
                let mut addresses: Vec<String> = service
                    .get_addresses_v4()
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                addresses.sort();
                found.insert(
                    host_id.clone(),
                    DiscoveredHost {
                        host_id,
                        name: service
                            .get_property_val_str("name")
                            .unwrap_or_default()
                            .to_owned(),
                        addresses,
                        port: service.port,
                        app_version: service.get_property_val_str("v").map(str::to_owned),
                        schema_version: service
                            .get_property_val_str("s")
                            .and_then(|s| s.parse().ok()),
                    },
                );
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = daemon.stop_browse(SERVICE_TYPE);
    let _ = daemon.shutdown();
    Ok(found.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_names_are_readable_short_and_unique_per_host() {
        let name = instance_name("Supérette El Amel", "0190f3a2-1111-7000-8000-000000000001");
        assert_eq!(name, "Supérette El Amel 0190f3a2");
        let arabic = instance_name("مغازة الأمل", "0190f3a2-1111-7000-8000-000000000001");
        assert!(arabic.starts_with("مغازة الأمل "));
        let long = instance_name(&"x".repeat(200), "0190f3a2-1111-7000-8000-000000000001");
        assert!(long.len() <= 63, "{} bytes", long.len());
        assert!(
            instance_name("A.B\\C", "0190f3a2")
                .chars()
                .all(|c| c != '.' && c != '\\')
        );
    }

    /// Needs multicast on the machine's interfaces: run on the reference PC (`-- --ignored`).
    #[test]
    #[ignore = "multicast on a real network interface"]
    fn a_main_pc_is_found_by_a_till() {
        let advertiser = Advertiser::start(
            "0190f3a2-1111-7000-8000-000000000001",
            "Supérette El Amel",
            7345,
            "0.5.0",
            6,
        )
        .unwrap();
        let found = browse(std::time::Duration::from_secs(4)).unwrap();
        let me = found
            .iter()
            .find(|h| h.host_id == "0190f3a2-1111-7000-8000-000000000001")
            .unwrap();
        assert_eq!(
            (me.port, me.name.as_str(), me.schema_version),
            (7345, "Supérette El Amel", Some(6))
        );
        assert!(!me.addresses.is_empty());
        advertiser.stop();
    }
}
