//! Labels (spec §9.3, LBL-001…013): templates, contents in the label language(s) with the price
//! the customer pays, weighed-goods labels, A4 sheet plans, the last print per product and the
//! reprint queue. Rendering to TSPL/ZPL/PNG is `pos-hw`'s; A4 sheets are HTML (print host).

pub mod build;
#[cfg(test)]
mod preview_lot_tests;
pub mod queue;
pub mod sheet;
pub mod template;
#[cfg(test)]
mod tests;

pub use build::*;
pub use queue::*;
pub use sheet::{LabelSheetCell, LabelSheetFormat, plan, sheet_formats};
pub use template::*;
