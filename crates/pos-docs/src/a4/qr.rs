//! The QR of a document number (DOC-016 for A4): a module matrix the template draws as SVG, so
//! printing needs no image library and no network.

use qrcode::{Color, EcLevel, QrCode};
use serde::{Deserialize, Serialize};
use specta::Type;

/// `Deserialize`: the till sends the BaridiMob QR back with the customer screen's frame (HW-042).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct QrMatrix {
    pub size: u32,
    /// `size × size` characters, `1` dark, `0` light, row by row from the top-start corner.
    pub modules: String,
}

pub fn qr(text: &str) -> Option<QrMatrix> {
    let code = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::M).ok()?;
    let size = u32::try_from(code.width()).ok()?;
    let modules = code
        .into_colors()
        .into_iter()
        .map(|c| if c == Color::Dark { '1' } else { '0' })
        .collect();
    Some(QrMatrix { size, modules })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_qr_encodes_the_number_as_a_square_matrix() {
        let q = qr("FA2026-000045").unwrap();
        assert_eq!(
            q.size, 21,
            "13 alphanumeric characters fit version 1 at level M"
        );
        assert_eq!(q.modules.len(), 21 * 21);
        assert!(q.modules.chars().all(|c| c == '0' || c == '1'));
        assert_eq!(&q.modules[..7], "1111111", "finder pattern top-start");
        assert!(qr("").is_some());
    }
}
