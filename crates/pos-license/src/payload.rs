//! The licence payload v1 and the canonical bytes its Ed25519 signature covers.

use crate::codes;
use crate::device::parse_code;
use pos_core::error::AppError;
use serde::{Deserialize, Serialize};
use specta::Type;

pub const VERSION: u32 = 1;
pub const DAY_MS: i64 = 86_400_000;
/// A trial works this many days from its issue, unless the owner gives more (at most
/// [`MAX_TRIAL_DAYS`]).
pub const TRIAL_DAYS: i64 = 7;
pub const MAX_TRIAL_DAYS: i64 = 30;
/// Prefixes the signed bytes, so a signature over a licence can never be one over another message.
pub const SIGNING_CONTEXT: &[u8] = b"DZPOS-LICENSE-v1\n";
/// How far this PC's clock may be behind the owner's when a key is activated (time zones, a slow
/// CMOS clock): more is `license_clock_behind`.
pub const ISSUE_SKEW_MS: i64 = DAY_MS;
pub const MAX_NAME_CHARS: usize = 80;
pub const MAX_PHONE_CHARS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LicenseKind {
    /// Works until `expires_at` (7 days after the issue by default).
    Trial,
    /// Never expires.
    Lifetime,
}

/// Who the licence was sold to, as the owner typed it. Shown on this PC's licence screen only:
/// never journaled, never logged (`Debug` prints nothing of it).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields)]
pub struct LicenseCustomer {
    pub name: Option<String>,
    pub phone: Option<String>,
}

impl std::fmt::Debug for LicenseCustomer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LicenseCustomer(..)")
    }
}

/// What the owner signs. Field names are the wire format (snake case); unknown fields are refused,
/// so a v1 app never accepts a promise it does not understand.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payload {
    pub v: u32,
    /// UUIDv7.
    pub id: String,
    pub kind: LicenseKind,
    /// The canonical `DZPOS-XXXX-XXXX-XXXX-XXXX` of the PC it was issued for.
    pub device_code: String,
    /// Unix milliseconds, the owner's clock.
    pub issued_at: i64,
    /// A trial's end; `None` for a lifetime licence.
    pub expires_at: Option<i64>,
    pub customer: Option<LicenseCustomer>,
    /// Reserved for later versions: always empty in v1.
    pub features: Vec<String>,
    /// Which of `keys::PUBLIC_KEYS` signed it.
    pub key_id: String,
}

impl std::fmt::Debug for Payload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Payload")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl Payload {
    /// JSON with sorted keys and no whitespace (`pos_core::audit::canonical_json`): the bytes signed
    /// after [`SIGNING_CONTEXT`].
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, AppError> {
        let value = serde_json::to_value(self)?;
        Ok(pos_core::audit::canonical_json(&value).into_bytes())
    }

    /// The v1 rules, checked by the owner's tool before signing and by the app after verifying.
    pub fn validate(&self) -> Result<(), AppError> {
        let id_ok =
            self.id.len() == 36 && self.id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-');
        let device_ok = parse_code(&self.device_code).as_deref() == Some(self.device_code.as_str());
        let dates_ok = self.issued_at > 0
            && match (self.kind, self.expires_at) {
                (LicenseKind::Lifetime, None) => true,
                (LicenseKind::Trial, Some(end)) => {
                    end > self.issued_at && end - self.issued_at <= MAX_TRIAL_DAYS * DAY_MS
                }
                _ => false,
            };
        let text_ok = |text: &Option<String>, max: usize, allowed: fn(char) -> bool| {
            text.as_ref()
                .is_none_or(|t| t.chars().count() <= max && t.chars().all(allowed))
        };
        let customer_ok = self.customer.as_ref().is_none_or(|c| {
            text_ok(&c.name, MAX_NAME_CHARS, |ch| !ch.is_control())
                && text_ok(&c.phone, MAX_PHONE_CHARS, |ch| {
                    ch.is_ascii_digit() || " +-.".contains(ch)
                })
        });
        let key_id_ok = (1..=16).contains(&self.key_id.len())
            && self
                .key_id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        let features_ok =
            self.features.len() <= 16 && self.features.iter().all(|f| (1..=32).contains(&f.len()));
        if self.v == VERSION
            && id_ok
            && device_ok
            && dates_ok
            && customer_ok
            && key_id_ok
            && features_ok
        {
            Ok(())
        } else {
            Err(AppError::conflict(codes::INVALID))
        }
    }

    /// A trial whose end the high-water reached. A lifetime licence never expires.
    pub fn expired_at(&self, high_water: i64) -> bool {
        self.expires_at.is_some_and(|end| high_water >= end)
    }
}

/// Whole days left before `expires_at`, rounded up, 0 once it passed; `None` without an end.
pub fn days_left(expires_at: Option<i64>, high_water: i64) -> Option<i64> {
    expires_at.map(|end| ((end - high_water).max(0) + DAY_MS - 1) / DAY_MS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit;

    #[test]
    fn canonical_bytes_sort_the_keys_and_drop_the_whitespace() {
        let payload = testkit::payload(LicenseKind::Trial, &testkit::device(), 1_790_000_000_000);
        let text = String::from_utf8(payload.canonical_bytes().unwrap()).unwrap();
        assert!(
            text.starts_with("{\"customer\":{\"name\":\"Supérette El Amel\""),
            "{text}"
        );
        let keys = [
            "\"customer\"",
            "\"device_code\"",
            "\"expires_at\"",
            "\"features\"",
            "\"id\"",
            "\"issued_at\"",
            "\"key_id\"",
            "\"kind\"",
            "\"v\"",
        ];
        let positions: Vec<usize> = keys.iter().map(|k| text.find(k).unwrap()).collect();
        assert!(
            positions.windows(2).all(|w| w[0] < w[1]),
            "sorted keys: {text}"
        );
        assert!(!text.contains(": ") && !text.contains(", ") && !text.contains('\n'));
        assert_eq!(
            payload.canonical_bytes().unwrap(),
            payload.canonical_bytes().unwrap()
        );
        assert!(text.contains("\"kind\":\"trial\""), "{text}");
    }

    #[test]
    fn the_v1_rules() {
        let device = testkit::device();
        let good = testkit::payload(LicenseKind::Trial, &device, 1_790_000_000_000);
        good.validate().unwrap();
        testkit::payload(LicenseKind::Lifetime, &device, 1_790_000_000_000)
            .validate()
            .unwrap();
        let broken: [fn(&mut Payload); 12] = [
            |p| p.v = 2,
            |p| p.id = "not-a-uuid".into(),
            |p| p.device_code = "DZPOS-UUUU-UUUU-UUUU-UUUU".into(),
            |p| p.expires_at = None,
            |p| p.kind = LicenseKind::Lifetime,
            |p| p.expires_at = Some(p.issued_at + (MAX_TRIAL_DAYS + 1) * DAY_MS),
            |p| p.expires_at = Some(p.issued_at),
            |p| p.issued_at = 0,
            |p| {
                p.customer = Some(LicenseCustomer {
                    name: Some("x".repeat(MAX_NAME_CHARS + 1)),
                    phone: None,
                })
            },
            |p| {
                p.customer = Some(LicenseCustomer {
                    name: None,
                    phone: Some("0550 abc".into()),
                })
            },
            |p| p.key_id = "K1".into(),
            |p| p.features = (0..17).map(|i| format!("f{i}")).collect(),
        ];
        for (index, breaks) in broken.into_iter().enumerate() {
            let mut payload = good.clone();
            breaks(&mut payload);
            assert_eq!(
                payload.validate().unwrap_err().code(),
                crate::codes::INVALID,
                "rule {index}"
            );
        }
    }

    #[test]
    fn days_left_counts_whole_days_and_reaches_zero() {
        let issued = 1_790_000_000_000;
        let payload = testkit::payload(LicenseKind::Trial, &testkit::device(), issued);
        let end = payload.expires_at;
        assert_eq!(end, Some(issued + 7 * DAY_MS));
        assert_eq!(days_left(end, issued), Some(7));
        assert_eq!(days_left(end, issued + 1), Some(7));
        assert_eq!(days_left(end, issued + 6 * DAY_MS + 1), Some(1));
        assert_eq!(days_left(end, issued + 7 * DAY_MS - 1), Some(1));
        assert!(!payload.expired_at(issued + 7 * DAY_MS - 1));
        assert_eq!(days_left(end, issued + 7 * DAY_MS), Some(0));
        assert!(payload.expired_at(issued + 7 * DAY_MS));
        assert_eq!(days_left(end, issued + 30 * DAY_MS), Some(0));
        assert_eq!(
            days_left(None, issued),
            None,
            "a lifetime licence has no end"
        );
    }

    #[test]
    fn debug_prints_neither_the_device_nor_the_customer() {
        let payload =
            testkit::payload(LicenseKind::Lifetime, &testkit::device(), 1_790_000_000_000);
        let printed = format!("{payload:?}");
        assert!(printed.contains(&payload.id), "{printed}");
        assert!(
            !printed.contains("DZPOS") && !printed.contains("El Amel") && !printed.contains("0550"),
            "{printed}"
        );
    }
}
