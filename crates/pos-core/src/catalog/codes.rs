//! Internal codes (PRD-003): references `P000001` and in-store EAN-13 barcodes starting with the
//! shop's prefix (setting `products.internal_ean_prefix`, GS1 range 200–299). Weight barcodes
//! (M4) use other prefixes of the 2x range; the settings screen says so.

use crate::error::AppError;
use rusqlite::Transaction;

pub fn ean13_check_digit(first12: &str) -> Option<u8> {
    if first12.len() != 12 || !first12.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let sum: u32 = first12
        .bytes()
        .enumerate()
        .map(|(i, b)| {
            let d = u32::from(b - b'0');
            if i % 2 == 0 { d } else { d * 3 }
        })
        .sum();
    u8::try_from((10 - sum % 10) % 10).ok()
}

/// A 13-digit code with a correct check digit. Non-ASCII input (a scanner on the wrong keyboard
/// layout, an imported cell) is refused before slicing, so it can never panic.
pub fn is_valid_ean13(code: &str) -> bool {
    code.len() == 13
        && code.is_ascii()
        && ean13_check_digit(&code[..12])
            .is_some_and(|d| code.as_bytes().get(12) == Some(&(b'0' + d)))
}

/// The next free in-store EAN-13 (counter `internal_ean`, never reset).
pub fn next_internal_ean13(tx: &Transaction<'_>) -> Result<String, AppError> {
    let prefix: String = crate::settings::get(tx, "products.internal_ean_prefix")?;
    for _ in 0..10_000 {
        let seq = crate::numbering::next_number(tx, "internal_ean", "", 0)?;
        let body = format!("{prefix}{seq:09}");
        let Some(check) = ean13_check_digit(&body) else {
            return Err(AppError::Internal(
                "internal barcode counter overflow".into(),
            ));
        };
        let code = format!("{body}{check}");
        let taken: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM product_barcodes WHERE barcode = ?1)",
            [&code],
            |r| r.get(0),
        )?;
        if !taken {
            return Ok(code);
        }
    }
    Err(AppError::Internal("no free internal barcode".into()))
}

/// The next free product reference `P000001`.
pub fn next_reference(tx: &Transaction<'_>) -> Result<String, AppError> {
    for _ in 0..10_000 {
        let code = crate::numbering::next_code(tx, "product_reference", "P", 6)?;
        let taken: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM products WHERE reference = ?1)",
            [&code],
            |r| r.get(0),
        )?;
        if !taken {
            return Ok(code);
        }
    }
    Err(AppError::Internal("no free product reference".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::{Money, Qty};
    use crate::testkit::shop;

    #[test]
    fn ean13_check_digits() {
        assert_eq!(ean13_check_digit("400638133393"), Some(1));
        assert_eq!(ean13_check_digit("613000000001"), Some(5));
        assert!(is_valid_ean13("4006381333931"));
        assert!(!is_valid_ean13("4006381333932"));
        assert_eq!(ean13_check_digit("12345"), None);
        // 13 bytes with a multi-byte character across byte 12 (AZERTY layout mix-up): refused,
        // never a panic on a char boundary.
        assert!(!is_valid_ean13("12345678901é"));
    }

    #[test]
    fn internal_codes_use_the_shop_prefix_and_skip_taken_ones() {
        let shop = shop();
        let mut input = crate::catalog::products::ProductInput::simple(
            "Olives",
            crate::catalog::UNIT_KG_ID,
            &shop.tva9,
            Money(90_000),
        );
        input.reference = Some("P000001".into());
        input.barcodes = vec![crate::catalog::products::Barcode {
            barcode: "2000000000015".into(),
            pack_qty: Qty(1_000),
            label: None,
        }];
        shop.product_with(input);
        let code = shop.db.write(|tx| next_internal_ean13(tx)).unwrap();
        assert_eq!(
            code, "2000000000022",
            "the first number is taken: the next one"
        );
        assert!(is_valid_ean13(&code));
        assert_eq!(shop.db.write(|tx| next_reference(tx)).unwrap(), "P000002");
    }
}
