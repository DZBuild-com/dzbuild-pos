//! Pairing codes (spec §4.6): the main PC shows six digits, the new till types them once. A code
//! lives only in the main PC's memory, as its SHA-256; it is single-use, valid 10 minutes, and
//! burnt at the fifth wrong try, so a replayed or guessed request finds nothing to redeem. The
//! server adds a per-address rate limit on top (Task 15).

use crate::audit::{self, Actor};
use crate::error::AppError;
use rusqlite::Transaction;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use specta::Type;
use std::fmt;

pub const CODE_TTL_MS: i64 = 10 * 60_000;
pub const MAX_ATTEMPTS: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IssuedCode {
    pub code: String,
    pub expires_at: i64,
}

struct Pending {
    hash: [u8; 32],
    expires_at: i64,
    attempts: u32,
}

#[derive(Default)]
pub struct PairingDesk {
    pending: Option<Pending>,
}

impl fmt::Debug for PairingDesk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PairingDesk")
            .field("open_until", &self.pending.as_ref().map(|p| p.expires_at))
            .finish()
    }
}

fn digest(code: &str) -> [u8; 32] {
    Sha256::digest(code.as_bytes()).into()
}

fn same(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter()
        .zip(b.iter())
        .fold(0_u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn random_code() -> Result<String, AppError> {
    loop {
        let mut bytes = [0_u8; 4];
        getrandom::fill(&mut bytes).map_err(|e| AppError::Internal(format!("random: {e}")))?;
        let n = u32::from_le_bytes(bytes);
        // Rejection sampling: 4 294 000 000 is a multiple of 1 000 000, so every code is as likely.
        if n < 4_294_000_000 {
            return Ok(format!("{:06}", n % 1_000_000));
        }
    }
}

impl PairingDesk {
    /// A new code; any previous one stops working.
    pub fn issue(&mut self, now: i64) -> Result<IssuedCode, AppError> {
        let code = random_code()?;
        let expires_at = now + CODE_TTL_MS;
        self.pending = Some(Pending {
            hash: digest(&code),
            expires_at,
            attempts: 0,
        });
        Ok(IssuedCode { code, expires_at })
    }

    pub fn redeem(&mut self, code: &str, now: i64) -> Result<(), AppError> {
        let Some(pending) = self.pending.as_mut() else {
            return Err(AppError::conflict("pairing_code_invalid"));
        };
        if now > pending.expires_at {
            self.pending = None;
            return Err(AppError::conflict("pairing_code_expired"));
        }
        if same(&pending.hash, &digest(code.trim())) {
            self.pending = None;
            return Ok(());
        }
        pending.attempts += 1;
        if pending.attempts >= MAX_ATTEMPTS {
            self.pending = None;
        }
        Err(AppError::conflict("pairing_code_invalid"))
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    pub fn open_until(&self, now: i64) -> Option<i64> {
        self.pending
            .as_ref()
            .map(|p| p.expires_at)
            .filter(|at| *at >= now)
    }
}

/// `lan.pairing_code_issued`, never the code itself.
pub fn journal_issued(
    tx: &Transaction<'_>,
    actor: &Actor,
    expires_at: i64,
) -> Result<(), AppError> {
    audit::append(
        tx,
        &actor.event(
            "lan.pairing_code_issued",
            "lan",
            None,
            json!({ "expiresAt": expires_at }),
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_790_593_200_000;

    #[test]
    fn codes_are_six_random_digits_one_at_a_time() {
        let mut desk = PairingDesk::default();
        let mut seen = std::collections::BTreeSet::new();
        let mut last = String::new();
        for _ in 0..50 {
            let issued = desk.issue(NOW).unwrap();
            assert_eq!(issued.code.len(), 6);
            assert!(issued.code.bytes().all(|b| b.is_ascii_digit()));
            assert_eq!(issued.expires_at, NOW + CODE_TTL_MS);
            last = issued.code.clone();
            seen.insert(issued.code);
        }
        assert!(seen.len() > 40, "random, not a counter");
        assert_eq!(
            format!("{desk:?}"),
            format!("PairingDesk {{ open_until: Some({}) }}", NOW + CODE_TTL_MS),
            "Debug never shows the code"
        );
        assert!(
            desk.redeem(&last, NOW).is_ok(),
            "the last code issued is the one open"
        );
    }

    #[test]
    fn a_pairing_code_works_once_and_a_replayed_request_is_refused() {
        let mut desk = PairingDesk::default();
        let code = desk.issue(NOW).unwrap().code;
        desk.redeem(&code, NOW + 1_000).unwrap();
        assert_eq!(
            desk.redeem(&code, NOW + 2_000).unwrap_err().code(),
            "pairing_code_invalid"
        );
        assert_eq!(desk.open_until(NOW), None);
    }

    #[test]
    fn five_wrong_codes_burn_the_code() {
        let mut desk = PairingDesk::default();
        let code = desk.issue(NOW).unwrap().code;
        let wrong = if code == "000000" { "000001" } else { "000000" };
        for _ in 0..(MAX_ATTEMPTS - 1) {
            assert_eq!(
                desk.redeem(wrong, NOW).unwrap_err().code(),
                "pairing_code_invalid"
            );
        }
        assert!(
            desk.open_until(NOW).is_some(),
            "four wrong tries: still open"
        );
        assert_eq!(
            desk.redeem(wrong, NOW).unwrap_err().code(),
            "pairing_code_invalid"
        );
        assert_eq!(desk.open_until(NOW), None, "burnt");
        assert_eq!(
            desk.redeem(&code, NOW).unwrap_err().code(),
            "pairing_code_invalid"
        );
    }

    #[test]
    fn an_expired_or_cancelled_code_is_refused() {
        let mut desk = PairingDesk::default();
        let code = desk.issue(NOW).unwrap().code;
        assert_eq!(
            desk.redeem(&code, NOW + CODE_TTL_MS + 1)
                .unwrap_err()
                .code(),
            "pairing_code_expired"
        );
        let code = desk.issue(NOW).unwrap().code;
        desk.cancel();
        assert_eq!(
            desk.redeem(&code, NOW).unwrap_err().code(),
            "pairing_code_invalid"
        );
        assert_eq!(
            desk.redeem("12 34", NOW).unwrap_err().code(),
            "pairing_code_invalid"
        );
    }
}
