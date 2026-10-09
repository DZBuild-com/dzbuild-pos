//! What the three copies and the journal mean together ([`reconcile`]) and what the licence
//! allows ([`evaluate`], [`LicenseState`]).

use crate::codes;
use crate::device::DeviceId;
use crate::keys::Keyring;
use crate::payload::{LicenseCustomer, LicenseKind, days_left};
use crate::record::{Record, Slot, TrialMark, read_slot};
use crate::store::Source;
use crate::verify::{Verified, verify};
use pos_core::error::AppError;
use serde::Serialize;
use specta::Type;

/// The licence in use, as the screens show it (never the key text).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Licence {
    pub id: String,
    pub kind: LicenseKind,
    pub issued_at: i64,
    pub expires_at: Option<i64>,
    pub customer: Option<LicenseCustomer>,
    pub activated_at: Option<i64>,
}

impl Licence {
    pub fn of(verified: &Verified, activated_at: Option<i64>) -> Licence {
        let p = &verified.payload;
        Licence {
            id: p.id.clone(),
            kind: p.kind,
            issued_at: p.issued_at,
            expires_at: p.expires_at,
            customer: p.customer.clone(),
            activated_at,
        }
    }

    pub fn days_left(&self, high_water: i64) -> Option<i64> {
        days_left(self.expires_at, high_water)
    }
}

/// How far this PC's clock may be behind the newest time it showed while a trial still writes (a
/// clock corrected by Windows' time service moves by seconds): more is `license_clock_behind`.
pub const CLOCK_TOLERANCE_MS: i64 = 10 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LicenseState {
    /// The demo shop: no licence needed.
    Demo,
    /// Nothing for this PC (never activated, or removed).
    Unlicensed,
    Active(Licence),
    /// A trial past its end.
    Expired(Licence),
    /// The stored key is refused now: revoked by this release (`license_revoked`) or signed by a
    /// key this release no longer trusts (`license_invalid`).
    Refused {
        code: &'static str,
    },
    /// Only another PC's records here: this PC changed (the owner re-issues).
    OtherDevice {
        previous: String,
    },
    Tampered,
    DeviceUnreadable,
}

impl LicenseState {
    pub fn allows_writes(&self) -> bool {
        matches!(self, LicenseState::Demo | LicenseState::Active(_))
    }

    /// The error a refused write answers (`None` while writes are allowed).
    pub fn refusal(&self) -> Option<AppError> {
        let code = match self {
            LicenseState::Demo | LicenseState::Active(_) => return None,
            LicenseState::Unlicensed => codes::REQUIRED,
            LicenseState::Expired(_) => codes::EXPIRED,
            LicenseState::Refused { code } => code,
            LicenseState::OtherDevice { .. } => codes::WRONG_DEVICE,
            LicenseState::Tampered => codes::TAMPERED,
            LicenseState::DeviceUnreadable => codes::DEVICE_UNREADABLE,
        };
        Some(AppError::conflict(code))
    }

    /// The state at `now` once the high-water reached `high_water` (final review): [`Self::at`],
    /// and an active trial whose clock is set back more than [`CLOCK_TOLERANCE_MS`] behind the
    /// newest time this PC showed refuses writes (`license_clock_behind`) until the date is set.
    /// A trial counts from that newest time: a clock set back again and again cannot freeze it.
    #[must_use]
    pub fn at_clock(&self, now: i64, high_water: i64) -> LicenseState {
        match self.at(high_water) {
            LicenseState::Active(licence)
                if licence.kind == LicenseKind::Trial
                    && now.saturating_add(CLOCK_TOLERANCE_MS) < high_water =>
            {
                LicenseState::Refused {
                    code: codes::CLOCK_BEHIND,
                }
            }
            other => other,
        }
    }

    /// The state once the high-water reached `high_water`: an active trial past its end is expired.
    #[must_use]
    pub fn at(&self, high_water: i64) -> LicenseState {
        match self {
            LicenseState::Active(licence)
                if licence.expires_at.is_some_and(|end| high_water >= end) =>
            {
                LicenseState::Expired(licence.clone())
            }
            other => other.clone(),
        }
    }

    pub fn licence(&self) -> Option<&Licence> {
        match self {
            LicenseState::Active(licence) | LicenseState::Expired(licence) => Some(licence),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Problem {
    Unreadable,
    Edited,
    TrialMismatch,
    JournalMismatch,
    KeyringChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceProblem {
    pub source: Source,
    pub problem: Problem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reconciled {
    /// The record to keep: the valid copy with the highest `rev`, with the earliest first trial of
    /// every copy and of the journal, and the newest `seenAt` of every copy. `None`: nothing of
    /// this PC anywhere.
    pub record: Option<Record>,
    /// Non-empty: tampered (nothing is healed then, the evidence stays).
    pub problems: Vec<SourceProblem>,
    /// Stores to write `record` to: missing, another PC's, or holding something else.
    pub heal: Vec<Source>,
    /// Another PC's code, when only its records were found.
    pub foreign: Option<String>,
}

pub fn reconcile(
    device: &DeviceId,
    texts: &[(Source, Option<String>)],
    journal_trial: Option<&TrialMark>,
) -> Reconciled {
    let slots: Vec<(Source, Slot)> = texts
        .iter()
        .map(|(source, text)| (*source, read_slot(text.as_deref(), device)))
        .collect();
    let mut problems = Vec::new();
    let mut foreign = None;
    let mut valid: Vec<(Source, &Record)> = Vec::new();
    for (source, slot) in &slots {
        match slot {
            Slot::Unreadable => problems.push(SourceProblem {
                source: *source,
                problem: Problem::Unreadable,
            }),
            Slot::Edited => problems.push(SourceProblem {
                source: *source,
                problem: Problem::Edited,
            }),
            Slot::Foreign { device_code } => {
                foreign.get_or_insert_with(|| device_code.clone());
            }
            Slot::Valid(record) => valid.push((*source, record)),
            Slot::Missing => {}
        }
    }
    let marks: Vec<&TrialMark> = valid
        .iter()
        .filter_map(|(_, record)| record.first_trial.as_ref())
        .collect();
    if let Some(first) = marks.first() {
        for (source, record) in &valid {
            if record
                .first_trial
                .as_ref()
                .is_some_and(|m| m.id != first.id)
            {
                problems.push(SourceProblem {
                    source: *source,
                    problem: Problem::TrialMismatch,
                });
            }
        }
        if journal_trial.is_some_and(|j| j.id != first.id) {
            problems.push(SourceProblem {
                source: Source::Journal,
                problem: Problem::JournalMismatch,
            });
        }
    }
    let earliest = marks
        .iter()
        .copied()
        .chain(journal_trial)
        .min_by_key(|m| m.activated_at)
        .cloned();
    let newest_seen = valid.iter().map(|(_, r)| r.seen_at).max();
    let record = match valid.iter().max_by_key(|(_, r)| r.rev) {
        Some((_, newest)) => Some(Record {
            first_trial: earliest,
            seen_at: newest_seen.unwrap_or(newest.seen_at),
            ..(*newest).clone()
        }),
        None => journal_trial.map(|mark| Record {
            first_trial: Some(mark.clone()),
            ..Record::new(device.code())
        }),
    };
    let heal = match (&record, problems.is_empty()) {
        (Some(record), true) => slots
            .iter()
            .filter(|(_, slot)| match slot {
                Slot::Missing | Slot::Foreign { .. } => true,
                Slot::Valid(held) => !held.same_content(record),
                Slot::Unreadable | Slot::Edited => false,
            })
            .map(|(source, _)| *source)
            .collect(),
        _ => Vec::new(),
    };
    Reconciled {
        record,
        problems,
        heal,
        foreign: if valid.is_empty() { foreign } else { None },
    }
}

/// What the reconciled record allows at `high_water`.
pub fn evaluate(
    reconciled: &Reconciled,
    device: &DeviceId,
    keyring: &Keyring,
    high_water: i64,
) -> LicenseState {
    if !reconciled.problems.is_empty() {
        return LicenseState::Tampered;
    }
    let Some(record) = &reconciled.record else {
        return match &reconciled.foreign {
            Some(previous) => LicenseState::OtherDevice {
                previous: previous.clone(),
            },
            None => LicenseState::Unlicensed,
        };
    };
    let Some(key) = record.key.as_deref() else {
        return LicenseState::Unlicensed;
    };
    match verify(key, keyring, device.code()) {
        Ok(verified) => {
            LicenseState::Active(Licence::of(&verified, record.activated_at)).at(high_water)
        }
        Err(error) => LicenseState::Refused {
            code: known(&error.code()),
        },
    }
}

/// The `'static` constant of a licence code (an unexpected code reads as `license_invalid`).
fn known(code: &str) -> &'static str {
    codes::ALL
        .into_iter()
        .find(|c| *c == code)
        .unwrap_or(codes::INVALID)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::DAY_MS;
    use crate::record::Record;
    use crate::testkit;
    use pos_core::time::now_ms;

    fn sealed(device: &DeviceId, change: impl FnOnce(&mut Record)) -> Option<String> {
        let mut record = Record::new(device.code());
        change(&mut record);
        Some(record.sealed(device).unwrap().to_text().unwrap())
    }

    fn texts(
        meta: Option<String>,
        vault: Option<String>,
        file: Option<String>,
    ) -> [(Source, Option<String>); 3] {
        [
            (Source::Meta, meta),
            (Source::Vault, vault),
            (Source::File, file),
        ]
    }

    fn mark(id: &str) -> TrialMark {
        TrialMark {
            id: id.into(),
            issued_at: 1_790_000_000_000,
            activated_at: 1_790_000_100_000,
        }
    }

    #[test]
    fn nothing_anywhere_is_unlicensed() {
        let device = testkit::device();
        let found = reconcile(&device, &texts(None, None, None), None);
        assert_eq!(
            found,
            Reconciled {
                record: None,
                problems: vec![],
                heal: vec![],
                foreign: None
            }
        );
        assert_eq!(
            evaluate(&found, &device, &testkit::keyring(), now_ms()),
            LicenseState::Unlicensed
        );
    }

    #[test]
    fn one_valid_copy_heals_the_two_missing_ones() {
        let device = testkit::device();
        let key = testkit::lifetime(&device, now_ms());
        let vault = sealed(&device, |r| {
            r.key = Some(key.clone());
            r.activated_at = Some(now_ms());
            r.rev = 3;
        });
        let found = reconcile(&device, &texts(None, vault, None), None);
        assert!(found.problems.is_empty());
        assert_eq!(found.heal, vec![Source::Meta, Source::File]);
        let state = evaluate(&found, &device, &testkit::keyring(), now_ms());
        assert!(
            matches!(&state, LicenseState::Active(l) if l.kind == LicenseKind::Lifetime),
            "{state:?}"
        );
        assert!(state.allows_writes() && state.refusal().is_none());
    }

    #[test]
    fn a_restored_older_database_loses_to_the_newer_copies_without_tampering() {
        let device = testkit::device();
        let now = now_ms();
        let trial = testkit::trial(&device, now - DAY_MS);
        let lifetime = testkit::lifetime(&device, now);
        let trial_id = verify(&trial, &testkit::keyring(), device.code())
            .unwrap()
            .payload
            .id;
        let old = sealed(&device, |r| {
            r.key = Some(trial.clone());
            r.first_trial = Some(TrialMark {
                id: trial_id.clone(),
                issued_at: now - DAY_MS,
                activated_at: now - DAY_MS,
            });
            r.rev = 1;
        });
        let new = sealed(&device, |r| {
            r.key = Some(lifetime.clone());
            r.first_trial = Some(TrialMark {
                id: trial_id.clone(),
                issued_at: now - DAY_MS,
                activated_at: now - DAY_MS,
            });
            r.rev = 3;
        });
        let found = reconcile(&device, &texts(old, new.clone(), new), None);
        assert!(found.problems.is_empty(), "{:?}", found.problems);
        assert_eq!(found.heal, vec![Source::Meta]);
        assert_eq!(
            found.record.as_ref().and_then(|r| r.key.clone()),
            Some(lifetime)
        );
        let state = evaluate(&found, &device, &testkit::keyring(), now);
        assert!(
            matches!(&state, LicenseState::Active(l) if l.kind == LicenseKind::Lifetime),
            "{state:?}"
        );
    }

    #[test]
    fn a_removed_licence_stays_removed_after_an_old_backup_is_restored() {
        let device = testkit::device();
        let key = testkit::lifetime(&device, now_ms());
        let old = sealed(&device, |r| {
            r.key = Some(key.clone());
            r.rev = 2;
        });
        let removed = sealed(&device, |r| r.rev = 4);
        let found = reconcile(&device, &texts(old, removed.clone(), removed), None);
        assert_eq!(found.heal, vec![Source::Meta]);
        assert_eq!(
            evaluate(&found, &device, &testkit::keyring(), now_ms()),
            LicenseState::Unlicensed
        );
    }

    #[test]
    fn a_copy_edited_by_hand_or_unreadable_is_tampered_and_nothing_heals() {
        let device = testkit::device();
        let good = sealed(&device, |r| {
            r.key = Some(testkit::lifetime(&device, now_ms()));
            r.rev = 2;
        });
        let mut edited: serde_json::Value = serde_json::from_str(good.as_deref().unwrap()).unwrap();
        edited["rev"] = serde_json::json!(9);
        let found = reconcile(
            &device,
            &texts(good, Some("{not json".into()), Some(edited.to_string())),
            None,
        );
        assert_eq!(
            found.problems,
            vec![
                SourceProblem {
                    source: Source::Vault,
                    problem: Problem::Unreadable
                },
                SourceProblem {
                    source: Source::File,
                    problem: Problem::Edited
                },
            ]
        );
        assert!(found.heal.is_empty(), "the evidence stays where it is");
        let state = evaluate(&found, &device, &testkit::keyring(), now_ms());
        assert_eq!(state, LicenseState::Tampered);
        assert_eq!(
            state.refusal().map(|e| e.code()).as_deref(),
            Some(crate::codes::TAMPERED)
        );
    }

    #[test]
    fn two_first_trials_or_a_journal_disagreeing_is_tampered() {
        let device = testkit::device();
        let a = sealed(&device, |r| r.first_trial = Some(mark("a")));
        let b = sealed(&device, |r| r.first_trial = Some(mark("b")));
        let found = reconcile(&device, &texts(a.clone(), b, None), None);
        assert_eq!(
            found.problems,
            vec![SourceProblem {
                source: Source::Vault,
                problem: Problem::TrialMismatch
            }]
        );
        let journal = mark("c");
        let found = reconcile(&device, &texts(a.clone(), a, None), Some(&journal));
        assert_eq!(
            found.problems,
            vec![SourceProblem {
                source: Source::Journal,
                problem: Problem::JournalMismatch
            }]
        );
    }

    #[test]
    fn records_of_another_pc_only_mean_the_pc_changed() {
        let device = testkit::device();
        let other = testkit::other_device();
        let theirs = sealed(&other, |r| {
            r.key = Some(testkit::lifetime(&other, now_ms()))
        });
        let found = reconcile(
            &device,
            &texts(theirs.clone(), theirs.clone(), theirs.clone()),
            None,
        );
        assert!(found.problems.is_empty() && found.heal.is_empty());
        let state = evaluate(&found, &device, &testkit::keyring(), now_ms());
        assert_eq!(
            state,
            LicenseState::OtherDevice {
                previous: other.code().into()
            }
        );
        assert_eq!(
            state.refusal().map(|e| e.code()).as_deref(),
            Some(crate::codes::WRONG_DEVICE)
        );
        // a backup of another PC restored here, beside this PC's own licence: healed, not tampered
        let mine = sealed(&device, |r| {
            r.key = Some(testkit::lifetime(&device, now_ms()));
            r.rev = 1;
        });
        let found = reconcile(&device, &texts(theirs, mine.clone(), mine), None);
        assert!(found.problems.is_empty() && found.foreign.is_none());
        assert_eq!(found.heal, vec![Source::Meta]);
    }

    #[test]
    fn the_journal_brings_back_a_wiped_first_trial() {
        let device = testkit::device();
        let journal = mark("a");
        let found = reconcile(&device, &texts(None, None, None), Some(&journal));
        assert_eq!(
            found.record.as_ref().and_then(|r| r.first_trial.clone()),
            Some(journal.clone())
        );
        assert_eq!(found.heal, vec![Source::Meta, Source::Vault, Source::File]);
        assert_eq!(
            evaluate(&found, &device, &testkit::keyring(), now_ms()),
            LicenseState::Unlicensed
        );
        // the earliest mark wins when the stores and the journal name the same trial
        let later = TrialMark {
            activated_at: journal.activated_at + 5_000,
            ..journal.clone()
        };
        let stored = sealed(&device, |r| r.first_trial = Some(later));
        let found = reconcile(&device, &texts(stored, None, None), Some(&journal));
        assert_eq!(found.record.and_then(|r| r.first_trial), Some(journal));
    }

    #[test]
    fn a_newer_record_format_is_rewritten_not_tampered() {
        let device = testkit::device();
        let mine = sealed(&device, |r| r.rev = 1);
        let found = reconcile(
            &device,
            &texts(Some("{\"v\":2}".into()), mine.clone(), mine),
            None,
        );
        assert!(found.problems.is_empty());
        assert_eq!(found.heal, vec![Source::Meta]);
    }

    #[test]
    fn the_trial_end_follows_the_high_water() {
        let device = testkit::device();
        let issued = now_ms();
        let key = testkit::trial(&device, issued);
        let stored = sealed(&device, |r| r.key = Some(key.clone()));
        let found = reconcile(&device, &texts(stored, None, None), None);
        let end = issued + 7 * DAY_MS;
        let active = evaluate(&found, &device, &testkit::keyring(), end - 1);
        assert_eq!(active.licence().and_then(|l| l.days_left(end - 1)), Some(1));
        assert!(active.allows_writes());
        let expired = evaluate(&found, &device, &testkit::keyring(), end);
        assert!(matches!(expired, LicenseState::Expired(_)), "{expired:?}");
        assert_eq!(
            expired.refusal().map(|e| e.code()).as_deref(),
            Some(crate::codes::EXPIRED)
        );
        assert!(
            matches!(active.at(end), LicenseState::Expired(_)),
            "`at` moves an active trial past its end"
        );
        assert_eq!(active.at(end - 1), active);
    }

    #[test]
    fn a_stored_key_signed_by_an_untrusted_key_is_refused() {
        let device = testkit::device();
        let stored = sealed(&device, |r| {
            r.key = Some(testkit::lifetime(&device, now_ms()))
        });
        let found = reconcile(&device, &texts(stored, None, None), None);
        let state = evaluate(
            &found,
            &device,
            &crate::keys::Keyring::production(),
            now_ms(),
        );
        assert_eq!(
            state,
            LicenseState::Refused {
                code: crate::codes::INVALID
            }
        );
        assert!(!state.allows_writes());
    }

    #[test]
    fn every_state_names_its_refusal() {
        let code = |s: LicenseState| s.refusal().map(|e| e.code());
        assert_eq!(code(LicenseState::Demo), None);
        assert_eq!(
            code(LicenseState::Unlicensed).as_deref(),
            Some(crate::codes::REQUIRED)
        );
        assert_eq!(
            code(LicenseState::Tampered).as_deref(),
            Some(crate::codes::TAMPERED)
        );
        assert_eq!(
            code(LicenseState::DeviceUnreadable).as_deref(),
            Some(crate::codes::DEVICE_UNREADABLE)
        );
        assert_eq!(
            code(LicenseState::Refused {
                code: crate::codes::REVOKED
            })
            .as_deref(),
            Some(crate::codes::REVOKED)
        );
        assert!(LicenseState::Demo.allows_writes());
    }
}
