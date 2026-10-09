//! 4-8 digit PINs hashed with Argon2id (spec §4.5). Parameters keep one verification
//! under ~150 ms on a Celeron J1900 while staying memory-hard.

use crate::error::AppError;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::{Algorithm, Argon2, Params, Version};

pub const PIN_MIN: usize = 4;
pub const PIN_MAX: usize = 8;
pub const ARGON2_M_KIB: u32 = 19_456;
pub const ARGON2_T: u32 = 2;
pub const ARGON2_P: u32 = 1;

pub fn validate_pin(pin: &str) -> Result<(), AppError> {
    let digits_only = pin.bytes().all(|b| b.is_ascii_digit());
    if !(PIN_MIN..=PIN_MAX).contains(&pin.len()) || !digits_only {
        return Err(AppError::validation("pin", "pin_format"));
    }
    if is_too_simple(pin) {
        return Err(AppError::validation("pin", "pin_too_simple"));
    }
    Ok(())
}

/// All digits equal (1111) or a straight run up or down (1234, 9876).
pub fn is_too_simple(pin: &str) -> bool {
    let d: Vec<i16> = pin
        .bytes()
        .map(|b| i16::from(b) - i16::from(b'0'))
        .collect();
    let same = d.windows(2).all(|w| w[0] == w[1]);
    let up = d.windows(2).all(|w| w[1] - w[0] == 1);
    let down = d.windows(2).all(|w| w[0] - w[1] == 1);
    same || up || down
}

fn argon2() -> Result<Argon2<'static>, AppError> {
    let params = Params::new(ARGON2_M_KIB, ARGON2_T, ARGON2_P, None)
        .map_err(|e| AppError::Internal(format!("argon2 params: {e}")))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// Validates then hashes a PIN into a PHC string with a random salt.
pub fn hash_pin(pin: &str) -> Result<String, AppError> {
    validate_pin(pin)?;
    let hash = argon2()?
        .hash_password(pin.as_bytes())
        .map_err(|e| AppError::Internal(format!("argon2 hash: {e}")))?;
    Ok(hash.to_string())
}

/// `Ok(false)` on a wrong PIN; `Err` only when the stored hash is unreadable.
pub fn verify_pin(pin: &str, hash: &str) -> Result<bool, AppError> {
    let parsed =
        PasswordHash::new(hash).map_err(|e| AppError::Internal(format!("stored PIN hash: {e}")))?;
    Ok(argon2()?.verify_password(pin.as_bytes(), &parsed).is_ok())
}

/// True for an Argon2id PHC string carrying a salt and an output, as [`hash_pin`] writes it.
/// The user store refuses anything else, so a plaintext PIN or a placeholder never lands there.
pub fn is_pin_hash(hash: &str) -> bool {
    hash.starts_with("$argon2id$")
        && PasswordHash::new(hash).is_ok_and(|h| h.salt.is_some() && h.hash.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn only_argon2id_phc_strings_are_pin_hashes() {
        assert!(is_pin_hash(&hash_pin("2580").unwrap()));
        let salt_and_output = "c29tZXNhbHRzb21lc2FsdA$ZmFrZWhhc2hmYWtlaGFzaGZha2VoYXNoZmFrZWhhc2g";
        assert!(is_pin_hash(&format!(
            "$argon2id$v=19$m=19456,t=2,p=1${salt_and_output}"
        )));
        for not_a_hash in [
            "",
            "2580",
            "hash",
            "$argon2id$",
            "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA",
            &format!("$argon2i$v=19$m=19456,t=2,p=1${salt_and_output}"),
            &format!("$pbkdf2-sha256$i=100000${salt_and_output}"),
        ] {
            assert!(!is_pin_hash(not_a_hash), "{not_a_hash}");
        }
    }

    #[test]
    fn pin_rules() {
        for ok in ["2580", "13579", "20262026", "7391"] {
            assert!(validate_pin(ok).is_ok(), "{ok}");
        }
        for (pin, code) in [
            ("123", "pin_format"),
            ("123456789", "pin_format"),
            ("12a4", "pin_format"),
            ("١٢٣٤", "pin_format"),
            ("1111", "pin_too_simple"),
            ("1234", "pin_too_simple"),
            ("9876", "pin_too_simple"),
            ("34567", "pin_too_simple"),
        ] {
            assert_eq!(validate_pin(pin).unwrap_err().code(), code, "{pin}");
        }
    }

    #[test]
    fn hash_is_argon2id_with_the_spec_parameters() {
        let first = hash_pin("2580").unwrap();
        let second = hash_pin("2580").unwrap();
        assert!(
            first.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
            "{first}"
        );
        assert_ne!(first, second, "salted");
        assert!(verify_pin("2580", &first).unwrap());
        assert!(!verify_pin("2581", &first).unwrap());
        assert_eq!(
            verify_pin("2580", "not-a-hash").unwrap_err().code(),
            "internal_error"
        );
    }

    #[test]
    #[ignore = "timing: cargo test -p pos-core --release pin_verify_is_fast -- --ignored (target < 150 ms on a Celeron J1900)"]
    fn pin_verify_is_fast() {
        let hash = hash_pin("2580").unwrap();
        let start = Instant::now();
        assert!(verify_pin("2580", &hash).unwrap());
        assert!(
            start.elapsed() < Duration::from_millis(150),
            "{:?}",
            start.elapsed()
        );
    }
}
