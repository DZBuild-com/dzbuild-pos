//! DZBuild POS domain core: money, database, audit journal, auth, settings, regulatory data.
//! No Tauri dependency; everything here is unit-tested.
#![forbid(unsafe_code)]

pub mod money;

pub mod appointments;
pub mod audit;
pub mod auth;
pub mod billing;
#[cfg(test)]
mod budgets;
pub mod butcher;
pub mod cash;
pub mod catalog;
pub mod cloud;
pub mod company;
#[cfg(test)]
mod completion_tests;
pub mod customers;
pub mod datapack;
pub mod datapack_update;
pub mod dataset;
pub mod db;
pub mod demo;
pub mod deposits;
pub mod documents;
pub mod error;
pub mod expenses;
pub mod fiscal;
pub mod flexy;
pub mod gift_vouchers;
pub mod health;
pub mod ids;
pub mod imports;
pub mod instalments;
#[cfg(test)]
mod instalments_tests;
pub mod jewellery;
#[cfg(test)]
pub(crate) mod jewellery_tests;
pub mod json;
pub mod labels;
pub mod lang;
pub mod ledger;
pub mod local;
pub mod loyalty;
#[cfg(test)]
mod loyalty_preview_tests;
#[cfg(test)]
mod loyalty_receipt_tests;
#[cfg(test)]
pub(crate) mod loyalty_tests;
pub mod numbering;
pub mod orders;
#[cfg(test)]
mod orders_deliver_tests;
#[cfg(test)]
mod orders_tests;
pub mod packs;
pub mod party;
pub mod payment_types;
pub mod payments;
pub mod pharmacy;
#[cfg(test)]
mod phones_tests;
pub mod privacy;
#[cfg(test)]
mod privacy_tests;
pub mod promotions;
pub mod purchases;
pub mod redact;
pub mod regulated;
pub mod reports;
pub mod restaurant;
pub mod returns;
pub mod sales;
#[cfg(test)]
mod serial_returns_tests;
#[cfg(test)]
mod service_tests;
pub mod service_tickets;
pub mod settings;
pub mod setup;
pub(crate) mod sqlenum;
pub mod staff;
#[cfg(test)]
pub(crate) mod staff_tests;
pub mod stock;
pub mod suppliers;
pub mod sync;
pub mod table;
pub mod tax;
pub mod terminal;
#[cfg(test)]
pub(crate) mod testkit;
pub mod till;
pub mod time;
pub mod updates;
pub mod vouchers;
pub mod wholesale;
pub mod zakat;

/// Crate name, checked by the workspace smoke test.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod smoke {
    #[test]
    fn crate_is_named() {
        assert_eq!(super::CRATE, "pos-core");
    }
}
