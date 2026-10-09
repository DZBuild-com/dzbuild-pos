//! Checking, activating and removing this PC's licence: the copies read and
//! reconciled, the high-water clock, the first-trial and downgrade rules, the journal rows written
//! in the same transaction as the database copy, then the Credential Manager and the file.
//! Blocking (the database, the Credential Manager): never on an async runtime thread.
//!
//! One at a time: [`refresh`], [`activate`] and [`remove`] each read the copies, then write them
//! back. Each one holds the `ENGINE` lock from its first read to its last write, so within this
//! process (the app runs as a single instance) a 10-minute check never writes back a record that
//! an activation or a removal on another thread replaced meanwhile, and two commands never
//! interleave their writes. No deadlock: the database's writer thread never calls the engine, and
//! no engine function calls another while it holds the lock. A panic while holding it poisons
//! nothing that matters (the lock guards no data): the next caller takes it anyway.

use crate::codes;
use crate::device::DeviceId;
use crate::keys::{Keyring, keyring_intact};
use crate::payload::{ISSUE_SKEW_MS, LicenseKind};
use crate::record::{Record, TrialMark};
use crate::state::{
    CLOCK_TOLERANCE_MS, Licence, LicenseState, Problem, Reconciled, SourceProblem, evaluate,
    reconcile,
};
use crate::store::{META_KEY, Source, read_texts, write_outside};
use crate::verify::verify;
use pos_core::audit::{self, Actor};
use pos_core::cloud::secrets::SecretStore;
use pos_core::db::{Db, meta_set};
use pos_core::error::AppError;
use rusqlite::OptionalExtension;
use serde_json::{Value, json};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Held by each entry point from its first read of the copies to its last write (module docs).
static ENGINE: Mutex<()> = Mutex::new(());

fn one_at_a_time() -> MutexGuard<'static, ()> {
    ENGINE.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Context<'a> {
    pub db: &'a Db,
    pub vault: &'a dyn SecretStore,
    pub file: &'a dyn SecretStore,
    pub device: &'a DeviceId,
    pub keyring: &'a Keyring,
    pub actor: &'a Actor,
    /// This PC's clock (`AppState::now` in the shell).
    pub now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub state: LicenseState,
    /// The newest instant this PC showed, for the shell's in-memory checks.
    pub high_water: i64,
}

struct Snapshot {
    reconciled: Reconciled,
    journal: Option<TrialMark>,
    high_water: i64,
}

fn snapshot(ctx: &Context<'_>) -> Result<Snapshot, AppError> {
    let texts = read_texts(ctx.db, ctx.vault, ctx.file)?;
    let journal = journal_trial(ctx.db, ctx.device.code())?;
    let reconciled = reconcile(ctx.device, &texts, journal.as_ref());
    let newest_row: Option<i64> = ctx
        .db
        .read(|c| Ok(c.query_row("SELECT MAX(at) FROM audit_log", [], |r| r.get(0))?))?;
    let seen = reconciled.record.as_ref().map_or(0, |r| r.seen_at);
    Ok(Snapshot {
        high_water: ctx.now.max(newest_row.unwrap_or(0)).max(seen),
        reconciled,
        journal,
    })
}

/// The journal's first trial activated on `device_code`: a witness the stores cannot
/// erase (deleting the row breaks the hash chain).
pub fn journal_trial(db: &Db, device_code: &str) -> Result<Option<TrialMark>, AppError> {
    let code = device_code.to_owned();
    db.read(move |c| {
        let payload: Option<String> = c
            .query_row(
                "SELECT payload FROM audit_log
                 WHERE action = 'license.activated'
                   AND json_extract(payload, '$.kind') = 'trial'
                   AND json_extract(payload, '$.deviceCode') = ?1
                 ORDER BY seq LIMIT 1",
                [&code],
                |r| r.get(0),
            )
            .optional()?;
        Ok(payload.and_then(|text| {
            let value: Value = serde_json::from_str(&text).ok()?;
            Some(TrialMark {
                id: value.get("licenseId")?.as_str()?.to_owned(),
                issued_at: value.get("issuedAt")?.as_i64()?,
                activated_at: value.get("activatedAt")?.as_i64()?,
            })
        }))
    })
}

fn journal_tampered(ctx: &Context<'_>, problems: &[SourceProblem]) -> Result<(), AppError> {
    let event = ctx.actor.event(
        "license.tampered",
        "license",
        None,
        json!({ "problems": problems }),
    );
    ctx.db.write(move |tx| audit::append(tx, &event).map(drop))
}

/// The check of the start and of every 10 minutes: reads the copies and the journal, journals
/// what changed (`license.repaired` for copies put back, `license.expired` once per licence,
/// `license.tampered` when `journal_tamper` — the shell asks once per run), writes the repaired
/// or advanced record everywhere, and says what the licence allows now.
pub fn refresh(ctx: &Context<'_>, journal_tamper: bool) -> Result<Outcome, AppError> {
    let _engine = one_at_a_time();
    if !keyring_intact() {
        if journal_tamper {
            journal_tampered(
                ctx,
                &[SourceProblem {
                    source: Source::Keyring,
                    problem: Problem::KeyringChanged,
                }],
            )?;
        }
        return Ok(Outcome {
            state: LicenseState::Tampered,
            high_water: ctx.now,
        });
    }
    let Snapshot {
        reconciled,
        high_water,
        ..
    } = snapshot(ctx)?;
    // A trial whose clock is set back refuses writes (final review), whatever the stores say.
    let state =
        evaluate(&reconciled, ctx.device, ctx.keyring, high_water).at_clock(ctx.now, high_water);
    if !reconciled.problems.is_empty() {
        if journal_tamper {
            journal_tampered(ctx, &reconciled.problems)?;
        }
        return Ok(Outcome { state, high_water });
    }
    let Some(mut record) = reconciled.record.clone() else {
        return Ok(Outcome { state, high_water });
    };
    // The newest time seen matters to everything but an active lifetime licence.
    let lifetime = matches!(&state, LicenseState::Active(l) if l.kind == LicenseKind::Lifetime);
    let advance = !lifetime && record.seen_at < high_water;
    let expired = match &state {
        LicenseState::Expired(licence) => Some((licence.id.clone(), licence.expires_at)),
        _ => None,
    };
    let write = !reconciled.heal.is_empty() || advance;
    if !write && expired.is_none() {
        return Ok(Outcome { state, high_water });
    }
    if write {
        record.seen_at = record.seen_at.max(high_water);
        record.rev += 1;
        record = record.sealed(ctx.device)?;
    }
    let meta_text = if write { Some(record.to_text()?) } else { None };
    let healed: Vec<&'static str> = reconciled.heal.iter().map(|s| s.as_str()).collect();
    let licence_id = state.licence().map(|l| l.id.clone());
    let actor = ctx.actor.clone();
    ctx.db.write(move |tx| {
        if let Some(text) = &meta_text {
            meta_set(tx, META_KEY, text)?;
        }
        if !healed.is_empty() {
            let event = actor.event(
                "license.repaired",
                "license",
                licence_id.as_deref(),
                json!({ "stores": healed }),
            );
            audit::append(tx, &event)?;
        }
        if let Some((id, expires_at)) = &expired {
            let journaled: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM audit_log WHERE action = 'license.expired' AND entity_id = ?1)",
                [id],
                |r| r.get(0),
            )?;
            if !journaled {
                let event = actor.event(
                    "license.expired",
                    "license",
                    Some(id),
                    json!({ "licenseId": id, "expiresAt": expires_at }),
                );
                audit::append(tx, &event)?;
            }
        }
        Ok(())
    })?;
    if write {
        write_outside(ctx.vault, ctx.file, &record)?;
    }
    Ok(Outcome { state, high_water })
}

/// Activates the key in `text` on this PC: verified (signature, device, revocation), then the
/// clock, first-trial and downgrade rules, then the record written everywhere with its
/// `license.activated` row. A refusal writes nothing. Re-activating the same key keeps its first
/// activation time; a tampered PC may activate again (fresh copies everywhere).
pub fn activate(ctx: &Context<'_>, text: &str) -> Result<Outcome, AppError> {
    let _engine = one_at_a_time();
    if !keyring_intact() {
        return Err(AppError::conflict(codes::TAMPERED));
    }
    let verified = verify(text, ctx.keyring, ctx.device.code())?;
    let payload = &verified.payload;
    if ctx.now + ISSUE_SKEW_MS < payload.issued_at {
        return Err(AppError::conflict(codes::CLOCK_BEHIND));
    }
    let Snapshot {
        reconciled,
        journal,
        high_water,
    } = snapshot(ctx)?;
    let current = reconciled
        .record
        .unwrap_or_else(|| Record::new(ctx.device.code()));
    // The journal's mark first: it is the witness the stores cannot rewrite.
    let first_trial = journal.or_else(|| current.first_trial.clone());
    let held = current
        .key
        .as_deref()
        .and_then(|key| verify(key, ctx.keyring, ctx.device.code()).ok());
    if payload.kind == LicenseKind::Trial {
        if first_trial
            .as_ref()
            .is_some_and(|mark| mark.id != payload.id)
        {
            return Err(AppError::conflict(codes::TRIAL_USED));
        }
        if held
            .as_ref()
            .is_some_and(|h| h.payload.kind == LicenseKind::Lifetime)
        {
            return Err(AppError::conflict(codes::DOWNGRADE));
        }
        if payload.expired_at(high_water) {
            return Err(AppError::conflict(codes::EXPIRED));
        }
        // A trial counts from the newest time this PC showed (final review).
        if ctx.now.saturating_add(CLOCK_TOLERANCE_MS) < high_water {
            return Err(AppError::conflict(codes::CLOCK_BEHIND));
        }
    }
    let same = held.as_ref().is_some_and(|h| h.payload.id == payload.id);
    let activated_at = match (same, current.activated_at) {
        (true, Some(at)) => at,
        _ => ctx.now,
    };
    let first_trial = first_trial.or_else(|| {
        (payload.kind == LicenseKind::Trial).then(|| TrialMark {
            id: payload.id.clone(),
            issued_at: payload.issued_at,
            activated_at,
        })
    });
    let record = Record {
        key: Some(verified.key.clone()),
        activated_at: Some(activated_at),
        first_trial,
        seen_at: current.seen_at.max(high_water),
        rev: current.rev + 1,
        ..Record::new(ctx.device.code())
    }
    .sealed(ctx.device)?;
    let replaced = held
        .as_ref()
        .filter(|_| !same)
        .map(|h| h.payload.id.clone());
    let event = ctx.actor.event(
        "license.activated",
        "license",
        Some(&payload.id),
        json!({
            "licenseId": payload.id,
            "kind": payload.kind,
            "deviceCode": payload.device_code,
            "issuedAt": payload.issued_at,
            "expiresAt": payload.expires_at,
            "activatedAt": activated_at,
            "replaced": replaced,
        }),
    );
    let meta_text = record.to_text()?;
    ctx.db.write(move |tx| {
        meta_set(tx, META_KEY, &meta_text)?;
        audit::append(tx, &event).map(drop)
    })?;
    write_outside(ctx.vault, ctx.file, &record)?;
    let state = LicenseState::Active(Licence::of(&verified, Some(activated_at)))
        .at_clock(ctx.now, high_water);
    Ok(Outcome { state, high_water })
}

/// Removes the licence in use from this PC (journaled `license.removed`); the first-trial mark
/// and the newest time seen stay. Removing nothing changes nothing.
pub fn remove(ctx: &Context<'_>) -> Result<Outcome, AppError> {
    let _engine = one_at_a_time();
    let Snapshot {
        reconciled,
        high_water,
        ..
    } = snapshot(ctx)?;
    let unlicensed = Outcome {
        state: LicenseState::Unlicensed,
        high_water,
    };
    let Some(current) = reconciled.record else {
        return Ok(unlicensed);
    };
    let Some(key) = current.key.as_deref() else {
        return Ok(unlicensed);
    };
    let removed = verify(key, ctx.keyring, ctx.device.code())
        .ok()
        .map(|v| (v.payload.id, v.payload.kind));
    let record = Record {
        key: None,
        activated_at: None,
        seen_at: current.seen_at.max(high_water),
        rev: current.rev + 1,
        ..current
    }
    .sealed(ctx.device)?;
    let (id, kind) = removed.unzip();
    let event = ctx.actor.event(
        "license.removed",
        "license",
        id.as_deref(),
        json!({ "licenseId": id, "kind": kind }),
    );
    let meta_text = record.to_text()?;
    ctx.db.write(move |tx| {
        meta_set(tx, META_KEY, &meta_text)?;
        audit::append(tx, &event).map(drop)
    })?;
    write_outside(ctx.vault, ctx.file, &record)?;
    Ok(unlicensed)
}
