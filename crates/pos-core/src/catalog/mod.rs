//! Catalogue (spec §5.1 M1): categories, units, tax rates, price levels, products with their
//! barcodes, prices and components, and the product search of spec §8. Every write is audited
//! in its own transaction; master data is archived, never deleted.

pub mod breaks;
pub mod bulk;
pub mod categories;
pub mod codes;
pub mod departments;
pub mod extras;
pub mod images;
pub mod normalize;
pub mod price_levels;
pub mod pricing;
pub mod products;
pub mod scale_codes;
pub mod scale_items;
pub mod search;
pub mod tax_rates;
pub mod units;
pub mod variant_grid;
pub mod variants;

use crate::datapack::Pack;
use crate::error::AppError;
use rusqlite::Transaction;

/// Units seeded by migration 0002.
pub const UNIT_PIECE_ID: &str = "00000000-0000-7000-8000-000000000101";
pub const UNIT_KG_ID: &str = "00000000-0000-7000-8000-000000000102";
pub const UNIT_GRAM_ID: &str = "00000000-0000-7000-8000-000000000103";
pub const UNIT_LITRE_ID: &str = "00000000-0000-7000-8000-000000000104";
pub const UNIT_METRE_ID: &str = "00000000-0000-7000-8000-000000000105";
pub const UNIT_SQUARE_METRE_ID: &str = "00000000-0000-7000-8000-000000000106";
pub const UNIT_BOX_ID: &str = "00000000-0000-7000-8000-000000000107";
pub const UNIT_CARTON_ID: &str = "00000000-0000-7000-8000-000000000108";

/// Price levels seeded by migration 0002 (PRC-001); `RETAIL_LEVEL_ID` is the default.
pub const RETAIL_LEVEL_ID: &str = "00000000-0000-7000-8000-000000000201";
pub const SEMI_WHOLESALE_LEVEL_ID: &str = "00000000-0000-7000-8000-000000000202";
pub const WHOLESALE_LEVEL_ID: &str = "00000000-0000-7000-8000-000000000203";
pub const RESELLER_LEVEL_ID: &str = "00000000-0000-7000-8000-000000000204";

/// Longest name in one language.
pub const NAME_MAX: usize = 120;
/// Longest reference, barcode, code or label.
pub const CODE_MAX: usize = 48;

/// Idempotent startup seeding that migrations cannot do: the VAT rates of the regulatory data
/// pack. Called at every start after `setup::bootstrap` (M1b) and by the test shop.
pub fn ensure_defaults(tx: &Transaction<'_>, pack: &Pack) -> Result<(), AppError> {
    tax_rates::sync_from_pack(tx, pack)?;
    Ok(())
}

/// The three trimmed names, at least one of them set (`name` / `required`), each at most
/// `NAME_MAX` characters without control characters (`nameAr` … / `too_long`, `value_invalid`).
pub(crate) fn clean_names(
    ar: &str,
    fr: &str,
    en: &str,
) -> Result<(String, String, String), AppError> {
    let names = [ar.trim(), fr.trim(), en.trim()];
    if names.iter().all(|n| n.is_empty()) {
        return Err(AppError::validation("name", "required"));
    }
    for (field, name) in ["nameAr", "nameFr", "nameEn"].into_iter().zip(names) {
        if name.chars().any(char::is_control) {
            return Err(AppError::validation(field, "value_invalid"));
        }
        if name.chars().count() > NAME_MAX {
            return Err(AppError::validation(field, "too_long"));
        }
    }
    Ok((
        names[0].to_owned(),
        names[1].to_owned(),
        names[2].to_owned(),
    ))
}

/// A trimmed optional text: empty becomes `None`; longer than `max` is `too_long`.
pub(crate) fn clean_opt(
    value: Option<&str>,
    field: &str,
    max: usize,
) -> Result<Option<String>, AppError> {
    let value = value.map(str::trim).filter(|v| !v.is_empty());
    if let Some(v) = value {
        if v.chars().count() > max {
            return Err(AppError::validation(field, "too_long"));
        }
        if v.chars().any(char::is_control) {
            return Err(AppError::validation(field, "value_invalid"));
        }
    }
    Ok(value.map(str::to_owned))
}

/// A required code (unit code, level code, location code): trimmed, 1..=`CODE_MAX` characters,
/// no spaces or control characters.
pub(crate) fn clean_code(value: &str, field: &str) -> Result<String, AppError> {
    let code = value.trim();
    if code.is_empty() {
        return Err(AppError::validation(field, "required"));
    }
    if code.chars().count() > CODE_MAX {
        return Err(AppError::validation(field, "too_long"));
    }
    if code.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(AppError::validation(field, "value_invalid"));
    }
    Ok(code.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn names_are_trimmed_and_checked() {
        assert_eq!(
            clean_names(" حليب ", "Lait ", "").unwrap(),
            ("حليب".into(), "Lait".into(), String::new())
        );
        assert_eq!(
            clean_names(" ", "", "")
                .unwrap_err()
                .payload()
                .field
                .as_deref(),
            Some("name")
        );
        let long = "x".repeat(NAME_MAX + 1);
        assert_eq!(clean_names("", &long, "").unwrap_err().code(), "too_long");
        assert_eq!(
            clean_names("", "a\u{7}", "").unwrap_err().code(),
            "value_invalid"
        );
        assert_eq!(clean_opt(Some("  "), "reference", 10).unwrap(), None);
        assert_eq!(
            clean_opt(Some("x".repeat(11).as_str()), "reference", 10)
                .unwrap_err()
                .code(),
            "too_long"
        );
        assert_eq!(clean_code(" kg ", "code").unwrap(), "kg");
        assert_eq!(
            clean_code("k g", "code").unwrap_err().code(),
            "value_invalid"
        );
        assert_eq!(clean_code("", "code").unwrap_err().code(), "required");
    }

    #[test]
    fn ensure_defaults_is_idempotent() {
        let db = Db::open_memory().unwrap();
        let pack = crate::datapack::bundled().unwrap();
        for _ in 0..2 {
            let pack = pack.clone();
            db.write(move |tx| ensure_defaults(tx, &pack)).unwrap();
        }
        let rates: i64 = db
            .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM tax_rates", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(rates, 3);
    }
}
