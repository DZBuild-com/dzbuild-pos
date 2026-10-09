//! DZBuild POS documents and import/export. Filled from milestone M1.

pub mod a4;
pub mod archive;
pub mod diagnostics;
pub mod fiscal_archive;
pub mod import;
pub mod letters;
pub mod locales;
pub mod media;
pub mod receipt;
pub mod settlement;
pub mod sheet;
pub mod slips;

/// Crate name, checked by the workspace smoke test.
pub const CRATE: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod smoke {
    #[test]
    fn crate_is_named() {
        assert_eq!(super::CRATE, "pos-docs");
    }
}
