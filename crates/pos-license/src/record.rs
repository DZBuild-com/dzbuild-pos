//! The licence as this PC keeps it, three times: the key, when it was activated here, the
//! first trial this PC ever used, the newest time seen, a revision (the highest wins) and a seal.
//! The seal is a SHA-256 over the record and the device secret: a copy edited by hand, or brought
//! from another PC, does not check. It is evidence on a PC the customer controls, not secrecy
//! (docs/release/licensing.md § Limits).

use crate::device::DeviceId;
use pos_core::error::AppError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const RECORD_VERSION: u32 = 1;
const SEAL_CONTEXT: &[u8] = b"DZPOS-LICENSE-RECORD-v1\x1f";

/// The first trial this PC used: written once, kept when the licence is removed or replaced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TrialMark {
    pub id: String,
    pub issued_at: i64,
    pub activated_at: i64,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Record {
    pub v: u32,
    pub device_code: String,
    /// The canonical `DZL1.` text of the licence in use; `None` once removed.
    pub key: Option<String>,
    /// When `key` was activated on this PC (this PC's clock).
    pub activated_at: Option<i64>,
    pub first_trial: Option<TrialMark>,
    /// The newest instant this PC showed.
    pub seen_at: i64,
    /// Raised by every write; the highest copy wins.
    pub rev: u64,
    #[serde(default)]
    pub seal: String,
}

impl std::fmt::Debug for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Record")
            .field("rev", &self.rev)
            .field("has_key", &self.key.is_some())
            .field("first_trial", &self.first_trial.as_ref().map(|m| &m.id))
            .finish_non_exhaustive()
    }
}

impl Record {
    pub fn new(device_code: &str) -> Record {
        Record {
            v: RECORD_VERSION,
            device_code: device_code.to_owned(),
            key: None,
            activated_at: None,
            first_trial: None,
            seen_at: 0,
            rev: 0,
            seal: String::new(),
        }
    }

    fn digest(&self, secret: &[u8; 32]) -> Result<String, AppError> {
        let unsealed = Record {
            seal: String::new(),
            ..self.clone()
        };
        let body = pos_core::audit::canonical_json(&serde_json::to_value(&unsealed)?);
        let mut hasher = Sha256::new();
        hasher.update(SEAL_CONTEXT);
        hasher.update(secret);
        hasher.update(body.as_bytes());
        Ok(hex::encode(hasher.finalize().as_slice()))
    }

    pub fn sealed(self, device: &DeviceId) -> Result<Record, AppError> {
        let seal = self.digest(device.secret())?;
        Ok(Record { seal, ..self })
    }

    pub fn to_text(&self) -> Result<String, AppError> {
        Ok(serde_json::to_string(self)?)
    }

    /// The same record whatever its seal.
    pub fn same_content(&self, other: &Record) -> bool {
        let strip = |r: &Record| Record {
            seal: String::new(),
            ..r.clone()
        };
        strip(self) == strip(other)
    }
}

/// What one store holds, for this PC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slot {
    Missing,
    /// Not a record: garbage, a cut file, a v1 record with unknown fields.
    Unreadable,
    /// A record of another PC (a backup restored from it, a copied file).
    Foreign {
        device_code: String,
    },
    /// A record of this PC whose seal does not check: edited by hand.
    Edited,
    Valid(Record),
}

pub fn read_slot(text: Option<&str>, device: &DeviceId) -> Slot {
    let Some(text) = text else {
        return Slot::Missing;
    };
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Slot::Unreadable;
    };
    // A record of a newer format (a newer version ran here, then this one was reinstalled): left
    // to that version, rewritten by this one rather than called tampered.
    if value
        .get("v")
        .and_then(Value::as_u64)
        .is_some_and(|v| v > u64::from(RECORD_VERSION))
    {
        return Slot::Missing;
    }
    let Ok(record) = serde_json::from_value::<Record>(value) else {
        return Slot::Unreadable;
    };
    if record.v != RECORD_VERSION {
        return Slot::Unreadable;
    }
    if record.device_code != device.code() {
        return Slot::Foreign {
            device_code: record.device_code,
        };
    }
    match record.digest(device.secret()) {
        Ok(expected) if expected == record.seal => Slot::Valid(record),
        _ => Slot::Edited,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit;

    fn sample() -> Record {
        Record {
            key: Some("DZL1.e30.AAAA".into()),
            activated_at: Some(1_790_000_000_000),
            first_trial: Some(TrialMark {
                id: "0199a0b0-0000-7000-8000-000000000001".into(),
                issued_at: 1_789_990_000_000,
                activated_at: 1_790_000_000_000,
            }),
            seen_at: 1_790_100_000_000,
            rev: 3,
            ..Record::new(testkit::device().code())
        }
    }

    #[test]
    fn a_sealed_record_reads_valid_and_round_trips() {
        let device = testkit::device();
        let sealed = sample().sealed(&device).unwrap();
        assert_eq!(sealed.seal.len(), 64);
        let text = sealed.to_text().unwrap();
        assert!(
            text.contains("\"firstTrial\"") && text.contains("\"seenAt\""),
            "{text}"
        );
        assert_eq!(read_slot(Some(&text), &device), Slot::Valid(sealed.clone()));
        assert!(sealed.same_content(&sample()), "the seal is not content");
        assert_eq!(read_slot(None, &device), Slot::Missing);
    }

    #[test]
    fn an_edit_a_garbage_or_another_pc_is_seen() {
        let device = testkit::device();
        let text = sample().sealed(&device).unwrap().to_text().unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
        value["seenAt"] = serde_json::json!(1_000);
        assert_eq!(read_slot(Some(&value.to_string()), &device), Slot::Edited);
        assert_eq!(read_slot(Some("{not json"), &device), Slot::Unreadable);
        assert_eq!(read_slot(Some(""), &device), Slot::Unreadable);
        let other = testkit::other_device();
        let theirs = Record {
            device_code: other.code().into(),
            ..sample()
        }
        .sealed(&other)
        .unwrap();
        assert_eq!(
            read_slot(Some(&theirs.to_text().unwrap()), &device),
            Slot::Foreign {
                device_code: other.code().into()
            }
        );
        // a newer format is left to its version, not called tampered
        assert_eq!(
            read_slot(Some("{\"v\":2,\"whatever\":true}"), &device),
            Slot::Missing
        );
    }

    #[test]
    fn debug_prints_no_key_and_no_seal() {
        let sealed = sample().sealed(&testkit::device()).unwrap();
        let printed = format!("{sealed:?}");
        assert!(
            !printed.contains("DZL1") && !printed.contains(&sealed.seal[..16]),
            "{printed}"
        );
        assert!(printed.contains("rev: 3"), "{printed}");
    }
}
