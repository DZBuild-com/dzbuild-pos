//! Field checks shared by customers and suppliers (CRM-001, PUR-001, CFG-004): Algerian phone
//! numbers, NIF/NIS lengths from the data pack, the 18-digit NIN, e-mail, wilaya, days.

use crate::company::is_valid_nif;
use crate::datapack::Pack;
use crate::error::AppError;
use chrono::NaiveDate;

pub const NAME_MAX: usize = 120;
pub const TEXT_MAX: usize = 200;

/// Arabic-Indic (٠–٩) and Persian (۰–۹) digits → 0–9 (UX-014).
pub fn western_digits(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{0660}'..='\u{0669}' => char::from(b'0' + (c as u32 - 0x0660) as u8),
            '\u{06F0}'..='\u{06F9}' => char::from(b'0' + (c as u32 - 0x06F0) as u8),
            other => other,
        })
        .collect()
}

fn trimmed(value: Option<&str>) -> Option<String> {
    value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// A required display name: trimmed, at most `NAME_MAX` characters, no control characters.
pub fn clean_name(value: &str) -> Result<String, AppError> {
    let name = value.trim();
    if name.is_empty() {
        return Err(AppError::validation("name", "required"));
    }
    if name.chars().count() > NAME_MAX {
        return Err(AppError::validation("name", "too_long"));
    }
    if name.chars().any(char::is_control) {
        return Err(AppError::validation("name", "value_invalid"));
    }
    Ok(name.to_owned())
}

/// Mobile `0[5-7]` + 8 digits (10 digits) or landline `0[1-4,9]` + 7 digits (9 digits).
pub fn is_valid_phone(digits: &str) -> bool {
    let b = digits.as_bytes();
    if !digits.bytes().all(|d| d.is_ascii_digit()) || b.first() != Some(&b'0') {
        return false;
    }
    matches!(
        (b.len(), b.get(1)),
        (10, Some(b'5' | b'6' | b'7')) | (9, Some(b'1'..=b'4' | b'9'))
    )
}

/// `+213 555-12-34-56`, `00213…`, `٠٥٥٥…` → `0555123456`; `None` when blank.
pub fn clean_phone(value: Option<&str>, field: &str) -> Result<Option<String>, AppError> {
    let Some(raw) = trimmed(value) else {
        return Ok(None);
    };
    let mut digits: String = western_digits(&raw)
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '.' | '/' | '(' | ')' | '\u{a0}'))
        .collect();
    if let Some(rest) = digits.strip_prefix("+213") {
        digits = format!("0{rest}");
    } else if let Some(rest) = digits.strip_prefix("00213") {
        digits = format!("0{rest}");
    }
    if is_valid_phone(&digits) {
        Ok(Some(digits))
    } else {
        Err(AppError::validation(field, "phone_invalid"))
    }
}

/// NIF or NIS: digits only (spaces dropped), a length of the data pack (`nif_lengths`).
pub fn clean_tax_id(
    value: Option<&str>,
    field: &str,
    pack: &Pack,
    code: &str,
) -> Result<Option<String>, AppError> {
    let Some(raw) = trimmed(value) else {
        return Ok(None);
    };
    let digits: String = western_digits(&raw)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if is_valid_nif(&digits, &pack.nif_lengths) {
        Ok(Some(digits))
    } else {
        Err(AppError::validation(field, code))
    }
}

/// NIN (numéro d'identification national): 18 digits.
pub fn clean_nin(value: Option<&str>, field: &str) -> Result<Option<String>, AppError> {
    let Some(raw) = trimmed(value) else {
        return Ok(None);
    };
    let digits: String = western_digits(&raw)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if digits.len() == 18 && digits.bytes().all(|d| d.is_ascii_digit()) {
        Ok(Some(digits))
    } else {
        Err(AppError::validation(field, "nin_invalid"))
    }
}

pub fn clean_email(value: Option<&str>, field: &str) -> Result<Option<String>, AppError> {
    let Some(email) = trimmed(value) else {
        return Ok(None);
    };
    let plausible = email.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty()
            && domain.contains('.')
            && !domain.starts_with('.')
            && !domain.ends_with('.')
    }) && !email.contains(char::is_whitespace)
        && email.chars().count() <= TEXT_MAX;
    if plausible {
        Ok(Some(email))
    } else {
        Err(AppError::validation(field, "email_invalid"))
    }
}

/// A wilaya code of the data pack (`"16"`).
pub fn clean_wilaya(
    value: Option<&str>,
    field: &str,
    pack: &Pack,
) -> Result<Option<String>, AppError> {
    let Some(code) = trimmed(value) else {
        return Ok(None);
    };
    let code = western_digits(&code);
    if pack.wilaya(&code).is_some() {
        Ok(Some(code))
    } else {
        Err(AppError::validation(field, "wilaya_invalid"))
    }
}

/// `YYYY-MM-DD`, a real calendar day.
pub fn clean_day(value: Option<&str>, field: &str) -> Result<Option<String>, AppError> {
    let Some(day) = trimmed(value) else {
        return Ok(None);
    };
    NaiveDate::parse_from_str(&day, "%Y-%m-%d")
        .map(|d| Some(d.format("%Y-%m-%d").to_string()))
        .map_err(|_| AppError::validation(field, "date_invalid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack() -> crate::datapack::Pack {
        crate::datapack::bundled().unwrap()
    }

    #[test]
    fn algerian_phones_are_normalised() {
        let ok = |raw: &str| clean_phone(Some(raw), "phone").unwrap();
        assert_eq!(ok("0555 12 34 56").as_deref(), Some("0555123456"));
        assert_eq!(ok("+213 555-12-34-56").as_deref(), Some("0555123456"));
        assert_eq!(ok("00213661234567").as_deref(), Some("0661234567"));
        assert_eq!(
            ok("٠٧٧٠١٢٣٤٥٦").as_deref(),
            Some("0770123456"),
            "Arabic-Indic digits"
        );
        assert_eq!(
            ok("021 23 45 67").as_deref(),
            Some("021234567"),
            "a landline"
        );
        assert_eq!(ok("  ").as_deref(), None);
        for bad in ["0555 12 34", "1555123456", "05551234567", "0555-ABC-123"] {
            assert_eq!(
                clean_phone(Some(bad), "phone").unwrap_err(),
                AppError::validation("phone", "phone_invalid"),
                "{bad}"
            );
        }
    }

    #[test]
    fn tax_ids_emails_wilayas_and_days_are_checked() {
        let pack = pack();
        assert_eq!(
            clean_tax_id(Some("0001 1600 1234 567"), "nif", &pack, "nif_invalid")
                .unwrap()
                .as_deref(),
            Some("000116001234567")
        );
        assert_eq!(
            clean_tax_id(Some("12345"), "nif", &pack, "nif_invalid").unwrap_err(),
            AppError::validation("nif", "nif_invalid")
        );
        assert_eq!(
            clean_nin(Some("1098 7654 3210 9876 54"), "nin")
                .unwrap()
                .as_deref(),
            Some("109876543210987654")
        );
        assert!(clean_nin(Some("12345678901234567"), "nin").is_err());
        assert!(clean_email(Some("karim@exemple.dz"), "email").is_ok());
        assert!(clean_email(Some("karim@"), "email").is_err());
        assert_eq!(
            clean_wilaya(Some("16"), "wilaya", &pack)
                .unwrap()
                .as_deref(),
            Some("16")
        );
        assert!(clean_wilaya(Some("99"), "wilaya", &pack).is_err());
        assert_eq!(
            clean_day(Some("2026-09-28"), "idsVerifiedAt")
                .unwrap()
                .as_deref(),
            Some("2026-09-28")
        );
        assert_eq!(
            clean_day(Some("28/09/2026"), "idsVerifiedAt").unwrap_err(),
            AppError::validation("idsVerifiedAt", "date_invalid")
        );
        assert_eq!(clean_name("  Karim  ").unwrap(), "Karim");
        assert_eq!(
            clean_name(" ").unwrap_err(),
            AppError::validation("name", "required")
        );
    }
}
