//! Label printers (spec §9.3, LBL-001…013, HW-013): one layout for every output, TSPL and ZPL
//! streams, label printer profiles. Text is always a bitmap shaped with the bundled Cairo (Arabic
//! included); barcodes are printer-native.

#[cfg(test)]
mod dropped_tests;
pub mod dump;
pub mod layout;
pub mod model;
pub mod profile;
pub mod tspl;
pub(crate) mod words;
pub mod zpl;

pub use dump::dump;
pub use layout::{
    BarcodeBox, LabelTexts, Placed, TextBox, TextRole, data_url, dropped, place, preview, texts,
    to_png,
};
pub use profile::{LabelDpi, LabelLanguage, LabelMedia, LabelPreset, LabelProfile};

use crate::bitmap::Bitmap;
use crate::error::HwError;
use crate::raster::Typesetter;
use model::{LabelGroup, LabelTemplate};

/// The bytes of `group` for a printer speaking `profile.language`.
pub fn job(
    group: &LabelGroup,
    profile: &LabelProfile,
    ts: &mut Typesetter,
) -> Result<Vec<u8>, HwError> {
    profile.validate()?;
    match profile.language {
        LabelLanguage::Tspl => tspl::job(group, profile, ts),
        LabelLanguage::Zpl => zpl::job(group, profile, ts),
    }
}

/// [`job`], noting in `warnings` each label whose layout leaves out the lot or the expiry date
/// it carries (décret 13-378): each label is laid out once, for the bytes and the warning.
pub fn job_noting(
    group: &LabelGroup,
    profile: &LabelProfile,
    ts: &mut Typesetter,
    warnings: &mut Vec<pos_core::labels::LabelWarning>,
) -> Result<Vec<u8>, HwError> {
    profile.validate()?;
    match profile.language {
        LabelLanguage::Tspl => tspl::job_noting(group, profile, ts, warnings),
        LabelLanguage::Zpl => zpl::job_noting(group, profile, ts, warnings),
    }
}

/// `columns` copies of one label's texts side by side (LBL-001 2-up stock).
pub(crate) fn row_bitmap(placed: &Placed, template: &LabelTemplate, dots_per_mm: u32) -> Bitmap {
    let width = template.row_width_dmm() * dots_per_mm / 10;
    let mut row = Bitmap::new(width, placed.height);
    for column in 0..u32::from(template.columns) {
        row.paste(&placed.text, column_x(template, column, dots_per_mm), 0);
    }
    row
}

pub(crate) fn column_x(template: &LabelTemplate, column: u32, dots_per_mm: u32) -> u32 {
    column * (u32::from(template.width_dmm) + u32::from(template.column_gap_dmm)) * dots_per_mm / 10
}

/// Barcode data both languages accept inside a quoted field.
pub(crate) fn printer_safe(data: &str) -> Result<&str, HwError> {
    if data.is_empty()
        || data
            .bytes()
            .any(|b| !(0x20..0x7F).contains(&b) || matches!(b, b'"' | b'\\' | b'^' | b'~'))
    {
        return Err(HwError::Barcode(format!(
            "{data:?} holds a character the printer reads as a command"
        )));
    }
    Ok(data)
}

/// "40", "38.1": millimetres from tenths.
pub(crate) fn mm(dmm: u32) -> String {
    if dmm.is_multiple_of(10) {
        format!("{}", dmm / 10)
    } else {
        format!("{}.{}", dmm / 10, dmm % 10)
    }
}

/// The digits a printer wants for an EAN/UPC (it adds the check digit itself).
pub(crate) fn body(data: &str, len: usize) -> Result<&str, HwError> {
    data.get(..len)
        .ok_or_else(|| HwError::Barcode(format!("{data:?} is too short")))
}
