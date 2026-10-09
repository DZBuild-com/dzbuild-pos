//! DZBuild POS hardware layer: ESC/POS receipts in text mode or as an Arabic raster, drivers,
//! discovery and the cash drawer (spec §9.1). Labels (TSPL/ZPL), scales, pole displays and serial
//! scanners since M4 (spec §9.3).

pub mod barcode;
pub mod bitmap;
pub mod codepage;
pub mod collector;
pub mod config;
pub mod device;
pub mod discover;
pub mod display;
pub mod driver;
pub mod error;
pub mod escpos;
pub mod fmt;
pub mod labels;
pub mod layout;
pub mod profile;
pub mod raster;
pub mod receipt;
pub mod registry;
pub mod render;
pub mod report;
pub mod scale;
pub mod scanner;
pub mod serial;
pub mod slips;
pub mod status;

#[cfg(test)]
mod budgets;
#[cfg(test)]
pub(crate) mod golden;
#[cfg(test)]
mod slips_tests;
#[cfg(test)]
pub(crate) mod testdata;

pub use error::{HwError, KNOWN_HW_CODES};
pub use registry::{PrintOutcome, Registry};
pub use status::{PrinterIssue, PrinterIssueKind, PrinterStatus};

/// Crate name, checked by the workspace smoke test.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod smoke {
    #[test]
    fn crate_is_named() {
        assert_eq!(super::CRATE, "pos-hw");
    }
}
