//! PIN login, lockout, sessions and manager override (spec §4.5).
//! Argon2 runs on the caller's thread; only the short bookkeeping writes use the writer.

use super::permission::Permission;
use super::pin;
use super::roles;
use super::users::{self, User};
use crate::audit::{self, Actor};
use crate::db::Db;
use crate::error::AppError;
use crate::lang::Lang;
use crate::terminal::Terminal;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;
use serde_json::{Map, Value, json};
use specta::Type;

pub const MAX_FAILED_ATTEMPTS: i64 = 5;
pub const LOCKOUT_MS: i64 = 5 * 60 * 1_000;

/// The signed-in user on one till. A `Session` lives only in Rust app state: it is sent to the
/// webview for display but never accepted back from it (hence no `Deserialize`). It is built
/// by [`build_session`] at login, and the command layer rebuilds it with [`refresh`] after any
/// users or roles edit, so that a changed role, name or language, or a deactivation, takes
/// effect at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub role_id: String,
    pub role_name_ar: String,
    pub role_name_fr: String,
    pub role_name_en: String,
    pub is_owner: bool,
    pub lang: Lang,
    pub permissions: Vec<Permission>,
    pub discount_limit_bp: i32,
    pub terminal_id: String,
    pub terminal_code: String,
    pub started_at: i64,
}

impl Session {
    pub fn has(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }

    pub fn actor(&self) -> Actor {
        Actor {
            user_id: Some(self.user_id.clone()),
            terminal_id: Some(self.terminal_id.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OverrideGrant {
    pub approver_id: String,
    pub approver_name: String,
    pub permission: Permission,
    pub granted_at: i64,
    /// The approver's own discount limit (10 000 for an owner): approving `sales.discount`
    /// lifts the requester's limit to this one, never further (PRM-004).
    pub discount_limit_bp: i32,
}

pub fn require(session: &Session, permission: Permission) -> Result<(), AppError> {
    if session.has(permission) {
        Ok(())
    } else {
        Err(AppError::Permission {
            permission: permission.as_str().to_owned(),
        })
    }
}

fn assemble(
    conn: &Connection,
    user: User,
    terminal_id: String,
    terminal_code: String,
    started_at: i64,
) -> Result<Session, AppError> {
    let permissions = roles::permissions_of(conn, &user.role_id)?;
    Ok(Session {
        user_id: user.id,
        username: user.username,
        display_name: user.display_name,
        role_id: user.role_id,
        role_name_ar: user.role_name_ar,
        role_name_fr: user.role_name_fr,
        role_name_en: user.role_name_en,
        is_owner: user.is_owner,
        lang: user.lang,
        permissions,
        discount_limit_bp: user.discount_limit_bp,
        terminal_id,
        terminal_code,
        started_at,
    })
}

pub fn build_session(
    conn: &Connection,
    user_id: &str,
    terminal: &Terminal,
    now: i64,
) -> Result<Session, AppError> {
    let user = users::get(conn, user_id)?;
    assemble(conn, user, terminal.id.clone(), terminal.code.clone(), now)
}

/// Rebuilds `session` from the database (role, permissions, names, language), keeping its
/// terminal and start time. `session_invalid` when the user was deactivated or is gone.
pub fn refresh(conn: &Connection, session: &Session) -> Result<Session, AppError> {
    let user = match users::get(conn, &session.user_id) {
        Ok(user) if user.active => user,
        Ok(_) | Err(AppError::NotFound { .. }) => return Err(AppError::auth("session_invalid")),
        Err(e) => return Err(e),
    };
    assemble(
        conn,
        user,
        session.terminal_id.clone(),
        session.terminal_code.clone(),
        session.started_at,
    )
}

struct Credentials {
    id: String,
    pin_hash: String,
    active: bool,
    locked_until: Option<i64>,
}

fn load_credentials(conn: &Connection, who: &str) -> Result<Option<Credentials>, AppError> {
    let creds = conn
        .query_row(
            "SELECT id, pin_hash, active, locked_until FROM users WHERE id = ?1 OR username = ?1",
            [who],
            |r| {
                Ok(Credentials {
                    id: r.get(0)?,
                    pin_hash: r.get(1)?,
                    active: r.get(2)?,
                    locked_until: r.get(3)?,
                })
            },
        )
        .optional()?;
    Ok(creds)
}

/// A lock holds while `now < locked_until`, and only when it ends at most one lockout away: a
/// lock dated further ahead was written by a clock that ran fast, and is stale like an expired
/// one (the next attempt clears it).
fn is_locked(locked_until: Option<i64>, now: i64) -> bool {
    locked_until.is_some_and(|until| now < until && until.saturating_sub(now) <= LOCKOUT_MS)
}

/// Why a PIN attempt was refused; audited as the `reason` of the failure row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    Inactive,
    Locked,
    WrongPin {
        attempts: i64,
        locked: bool,
    },
    /// Override only: the PIN matched but the approver's role lacks the permission.
    LacksPermission,
    /// Override only: the approver is the requester.
    SelfApproval,
}

impl Refusal {
    fn error(self) -> AppError {
        match self {
            Refusal::Inactive => AppError::auth("user_inactive"),
            Refusal::Locked | Refusal::WrongPin { locked: true, .. } => {
                AppError::auth("too_many_attempts")
            }
            Refusal::WrongPin { locked: false, .. } => AppError::auth("invalid_pin"),
            Refusal::LacksPermission => AppError::auth("approver_lacks_permission"),
            Refusal::SelfApproval => AppError::auth("override_self_not_allowed"),
        }
    }

    fn details(self) -> Map<String, Value> {
        let mut details = Map::new();
        let reason = match self {
            Refusal::Inactive => "inactive",
            Refusal::Locked => "locked",
            Refusal::WrongPin { attempts, locked } => {
                details.insert("attempts".into(), json!(attempts));
                details.insert("locked".into(), json!(locked));
                "invalid_pin"
            }
            Refusal::LacksPermission => "lacks_permission",
            Refusal::SelfApproval => "self_approval",
        };
        details.insert("reason".into(), json!(reason));
        details
    }
}

fn blocked(active: bool, locked_until: Option<i64>, now: i64) -> Option<Refusal> {
    if !active {
        Some(Refusal::Inactive)
    } else if is_locked(locked_until, now) {
        Some(Refusal::Locked)
    } else {
        None
    }
}

/// What a PIN is checked for; it decides how a refusal is audited.
#[derive(Debug, Clone)]
enum Purpose {
    /// Login or re-confirmation: `auth.login_failed`, by the user themself.
    SignIn,
    /// Manager override: `auth.override_failed`, by the requester, naming the approver.
    Override {
        requester: Actor,
        permission: Permission,
    },
}

/// A PIN attempt from one till.
#[derive(Debug, Clone)]
struct Attempt {
    purpose: Purpose,
    terminal_id: String,
    now: i64,
}

impl Attempt {
    /// Audits a refusal of `user_id`'s PIN. Unknown users are never audited: no row to name.
    fn audit_refusal(
        &self,
        tx: &Transaction<'_>,
        user_id: &str,
        refusal: Refusal,
    ) -> Result<(), AppError> {
        let mut payload = refusal.details();
        let event = match &self.purpose {
            Purpose::SignIn => Actor {
                user_id: Some(user_id.to_owned()),
                terminal_id: Some(self.terminal_id.clone()),
            }
            .event("auth.login_failed", "user", Some(user_id), payload.into()),
            Purpose::Override {
                requester,
                permission,
            } => {
                payload.insert("approverId".into(), json!(user_id));
                payload.insert("permission".into(), json!(permission.as_str()));
                requester.event(
                    "auth.override_failed",
                    "user",
                    Some(user_id),
                    payload.into(),
                )
            }
        };
        audit::append(tx, &event)?;
        Ok(())
    }
}

/// A refusal inside `Ok`: its audit row and counters must commit, so the transaction cannot
/// fail with it. An outer `Err` is a real failure and rolls everything back.
type Settled<T> = Result<Result<T, AppError>, AppError>;

/// Counts a wrong PIN; the fifth in a row locks the user for [`LOCKOUT_MS`]. Only called when no
/// lock holds, so the `locked_until = NULL` drops a stale lock and nothing else.
fn count_failure(tx: &Transaction<'_>, user_id: &str, now: i64) -> Result<Refusal, AppError> {
    let attempts: i64 = tx.query_row(
        "UPDATE users SET failed_attempts = failed_attempts + 1, locked_until = NULL WHERE id = ?1
         RETURNING failed_attempts",
        [user_id],
        |r| r.get(0),
    )?;
    let locked = attempts >= MAX_FAILED_ATTEMPTS;
    if locked {
        tx.execute(
            "UPDATE users SET failed_attempts = 0, locked_until = ?2 WHERE id = ?1",
            params![user_id, now.saturating_add(LOCKOUT_MS)],
        )?;
    }
    Ok(Refusal::WrongPin { attempts, locked })
}

/// Settles a checked PIN inside the writer transaction. Argon2 ran outside it, so the user's
/// state is read again here: a lock or a deactivation that appeared meanwhile wins, and a lock
/// is never cleared while it holds. A match resets the failure count (and a stale lock).
fn settle(
    tx: &Transaction<'_>,
    attempt: &Attempt,
    user_id: &str,
    pin_matches: bool,
) -> Settled<()> {
    let (active, locked_until): (bool, Option<i64>) = tx.query_row(
        "SELECT active, locked_until FROM users WHERE id = ?1",
        [user_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let refusal = match blocked(active, locked_until, attempt.now) {
        Some(refusal) => refusal,
        None if pin_matches => {
            tx.execute(
                "UPDATE users SET failed_attempts = 0, locked_until = NULL WHERE id = ?1",
                [user_id],
            )?;
            return Ok(Ok(()));
        }
        None => count_failure(tx, user_id, attempt.now)?,
    };
    attempt.audit_refusal(tx, user_id, refusal)?;
    Ok(Err(refusal.error()))
}

/// Checks `pin` for `who` (id or username). On a match, `on_success` runs in the same writer
/// transaction that re-checks the lock and resets the failure count; its own `Ok(Err(_))`
/// refusals commit too. A locked or inactive user's PIN is not even verified.
fn check_pin<T, F>(
    db: &Db,
    attempt: Attempt,
    who: &str,
    pin: &str,
    on_success: F,
) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce(&Transaction<'_>, &Attempt, &str) -> Settled<T> + Send + 'static,
{
    let who = who.to_owned();
    let creds = db
        .read(move |c| load_credentials(c, &who))?
        .ok_or_else(|| AppError::auth("invalid_credentials"))?;
    if let Some(refusal) = blocked(creds.active, creds.locked_until, attempt.now) {
        let id = creds.id;
        db.write(move |tx| attempt.audit_refusal(tx, &id, refusal))?;
        return Err(refusal.error());
    }
    let pin_matches = pin::verify_pin(pin, &creds.pin_hash)?;
    db.write(move |tx| {
        if let Err(refused) = settle(tx, &attempt, &creds.id, pin_matches)? {
            return Ok(Err(refused));
        }
        on_success(tx, &attempt, &creds.id)
    })?
}

pub fn login(db: &Db, terminal: &Terminal, who: &str, pin: &str) -> Result<Session, AppError> {
    login_at(db, terminal, who, pin, now_ms())
}

/// [`login`] at a given time. `now` must come from this PC's clock (`now_ms()`), never from
/// the webview: it decides whether a lock still holds.
pub fn login_at(
    db: &Db,
    terminal: &Terminal,
    who: &str,
    pin: &str,
    now: i64,
) -> Result<Session, AppError> {
    let attempt = Attempt {
        purpose: Purpose::SignIn,
        terminal_id: terminal.id.clone(),
        now,
    };
    let terminal = terminal.clone();
    check_pin(db, attempt, who, pin, move |tx, _, user_id| {
        tx.execute(
            "UPDATE users SET last_login_at = ?2 WHERE id = ?1",
            params![user_id, now],
        )?;
        let session = build_session(tx, user_id, &terminal, now)?;
        audit::append(
            tx,
            &session.actor().event(
                "auth.login",
                "user",
                Some(user_id),
                json!({ "terminal": terminal.code }),
            ),
        )?;
        Ok(Ok(session))
    })
}

pub fn logout(db: &Db, session: &Session) -> Result<(), AppError> {
    let session = session.clone();
    db.write(move |tx| {
        audit::append(
            tx,
            &session
                .actor()
                .event("auth.logout", "user", Some(&session.user_id), json!({})),
        )?;
        Ok(())
    })
}

/// Re-checks the logged-in user's PIN before a sensitive action (restore). A match resets the
/// failure count. `now` must come from this PC's clock, never from the webview.
pub fn confirm_pin(
    db: &Db,
    terminal: &Terminal,
    session: &Session,
    pin: &str,
    now: i64,
) -> Result<(), AppError> {
    let attempt = Attempt {
        purpose: Purpose::SignIn,
        terminal_id: terminal.id.clone(),
        now,
    };
    check_pin(db, attempt, &session.user_id, pin, |_, _, _| Ok(Ok(())))
}

/// A user holding `permission` approves a protected action for the requester; the audit row
/// records both people, and so does `auth.override_failed` for every refusal. Nobody approves
/// their own request (`override_self_not_allowed`). A wrong PIN counts toward the approver's
/// lockout. `now` must come from this PC's clock, never from the webview.
pub fn manager_override(
    db: &Db,
    terminal: &Terminal,
    requester: &Session,
    approver_id: &str,
    pin: &str,
    permission: Permission,
    now: i64,
) -> Result<OverrideGrant, AppError> {
    let attempt = Attempt {
        purpose: Purpose::Override {
            requester: requester.actor(),
            permission,
        },
        terminal_id: terminal.id.clone(),
        now,
    };
    let terminal = terminal.clone();
    let requester_id = requester.user_id.clone();
    let requester = requester.actor();
    check_pin(
        db,
        attempt,
        approver_id,
        pin,
        move |tx, attempt, approver| {
            if approver == requester_id {
                attempt.audit_refusal(tx, approver, Refusal::SelfApproval)?;
                return Ok(Err(Refusal::SelfApproval.error()));
            }
            let approver_session = build_session(tx, approver, &terminal, now)?;
            if !approver_session.has(permission) {
                attempt.audit_refusal(tx, approver, Refusal::LacksPermission)?;
                return Ok(Err(Refusal::LacksPermission.error()));
            }
            audit::append(
                tx,
                &requester.event(
                    "auth.override",
                    "user",
                    Some(approver),
                    json!({ "approverId": approver, "permission": permission.as_str() }),
                ),
            )?;
            Ok(Ok(grant_of(approver_session, permission, now)))
        },
    )
}

/// The grant an approver gives: their name and their own discount limit (10 000 for an owner).
fn grant_of(approver: Session, permission: Permission, granted_at: i64) -> OverrideGrant {
    OverrideGrant {
        discount_limit_bp: if approver.is_owner {
            10_000
        } else {
            approver.discount_limit_bp.clamp(0, 10_000)
        },
        approver_id: approver.user_id,
        approver_name: approver.display_name,
        permission,
        granted_at,
    }
}

/// A manager's approval a till's user brings to a host service (M5; Task 20 review): the till
/// checked the approver's PIN when it was given ([`manager_override`] there). This PC accepts it
/// only from an active user of its own, other than the requester, whose role holds `permission`
/// here, and rebuilds it from its own database (the approver's name and discount limit); `None`
/// otherwise, so the operation is judged on the requester's own rights.
pub fn confirm_grant(
    conn: &Connection,
    requester_id: &str,
    approver_id: &str,
    permission: Permission,
    granted_at: i64,
) -> Result<Option<OverrideGrant>, AppError> {
    if approver_id == requester_id {
        return Ok(None);
    }
    let user = match users::get(conn, approver_id) {
        Ok(user) if user.active => user,
        Ok(_) | Err(AppError::NotFound { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };
    let approver = assemble(conn, user, String::new(), String::new(), granted_at)?;
    Ok(approver
        .has(permission)
        .then(|| grant_of(approver, permission, granted_at)))
}

pub fn change_language(db: &Db, session: &Session, lang: Lang) -> Result<Session, AppError> {
    let session = session.clone();
    db.write(move |tx| {
        users::set_language(tx, &session.actor(), &session.user_id, lang)?;
        Ok(Session { lang, ..session })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::pin::hash_pin;
    use crate::auth::roles::{self, CASHIER_ROLE_ID, MANAGER_ROLE_ID, OWNER_ROLE_ID};
    use crate::auth::users::{NewUser, UserUpdate};

    const NOW: i64 = 1_790_596_800_000;

    struct Fixture {
        db: Db,
        terminal: Terminal,
        owner: User,
        manager: User,
        cashier: User,
    }

    fn add(db: &Db, name: &str, role: &str, pin: &str) -> User {
        let hash = hash_pin(pin).unwrap();
        let input = NewUser {
            username: None,
            display_name: name.into(),
            pin: pin.into(),
            role_id: role.into(),
            lang: Lang::Fr,
            discount_limit_bp: 0,
        };
        db.write(move |tx| users::create(tx, &Actor::default(), &input, &hash))
            .unwrap()
    }

    fn fixture() -> Fixture {
        let db = Db::open_memory().unwrap();
        let terminal = db
            .write(|tx| {
                roles::seed_system_roles(tx)?;
                crate::terminal::ensure_local(tx)
            })
            .unwrap();
        let owner = add(&db, "Amine Benali", OWNER_ROLE_ID, "2580");
        let manager = add(&db, "Karima Saadi", MANAGER_ROLE_ID, "7391");
        let cashier = add(&db, "Yacine", CASHIER_ROLE_ID, "4826");
        Fixture {
            db,
            terminal,
            owner,
            manager,
            cashier,
        }
    }

    fn actions(db: &Db) -> Vec<String> {
        db.read(|c| {
            let mut s = c.prepare("SELECT action FROM audit_log ORDER BY seq")?;
            let v = s
                .query_map([], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            Ok(v)
        })
        .unwrap()
    }

    /// `(user_id, payload)` of every audit row with `action`, oldest first.
    fn rows(db: &Db, action: &'static str) -> Vec<(Option<String>, Value)> {
        db.read(move |c| {
            let mut s =
                c.prepare("SELECT user_id, payload FROM audit_log WHERE action = ?1 ORDER BY seq")?;
            let v = s
                .query_map([action], |r| {
                    Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
        .unwrap()
        .into_iter()
        .map(|(user, payload)| (user, serde_json::from_str(&payload).unwrap()))
        .collect()
    }

    fn reasons(db: &Db, action: &'static str) -> Vec<String> {
        rows(db, action)
            .into_iter()
            .map(|(_, payload)| payload["reason"].as_str().unwrap().to_owned())
            .collect()
    }

    /// `(failed_attempts, locked_until)` of a user.
    fn lock_state(db: &Db, id: &str) -> (i64, Option<i64>) {
        let id = id.to_owned();
        db.read(move |c| {
            Ok(c.query_row(
                "SELECT failed_attempts, locked_until FROM users WHERE id = ?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .unwrap()
    }

    fn set_locked_until(db: &Db, id: &str, until: Option<i64>) {
        let id = id.to_owned();
        db.write(move |tx| {
            tx.execute(
                "UPDATE users SET locked_until = ?2 WHERE id = ?1",
                params![id, until],
            )?;
            Ok(())
        })
        .unwrap();
    }

    fn sign_in(terminal: &Terminal, now: i64) -> Attempt {
        Attempt {
            purpose: Purpose::SignIn,
            terminal_id: terminal.id.clone(),
            now,
        }
    }

    fn override_as(
        f: &Fixture,
        requester: &Session,
        approver: &str,
        pin: &str,
    ) -> Result<OverrideGrant, AppError> {
        manager_override(
            &f.db,
            &f.terminal,
            requester,
            approver,
            pin,
            Permission::SalesDiscount,
            NOW,
        )
    }

    #[test]
    fn login_returns_the_role_permissions() {
        let f = fixture();
        let owner = login_at(&f.db, &f.terminal, &f.owner.id, "2580", NOW).unwrap();
        assert!(owner.is_owner);
        assert_eq!(owner.permissions.len(), Permission::ALL.len());
        assert_eq!(
            (owner.terminal_code.as_str(), owner.started_at),
            ("C1", NOW)
        );
        let cashier = login_at(&f.db, &f.terminal, &f.cashier.id, "4826", NOW).unwrap();
        assert!(cashier.has(Permission::SalesCreate) && !cashier.has(Permission::SalesDiscount));
        assert_eq!(
            actions(&f.db).iter().filter(|a| *a == "auth.login").count(),
            2
        );
    }

    #[test]
    fn login_by_username_is_case_insensitive() {
        let f = fixture();
        let session = login_at(&f.db, &f.terminal, "AMINE.BENALI", "2580", NOW).unwrap();
        assert_eq!(session.user_id, f.owner.id);
    }

    #[test]
    fn five_wrong_pins_lock_the_user() {
        let f = fixture();
        for _ in 0..4 {
            assert_eq!(
                login_at(&f.db, &f.terminal, &f.cashier.id, "0000", NOW)
                    .unwrap_err()
                    .code(),
                "invalid_pin"
            );
        }
        assert_eq!(
            login_at(&f.db, &f.terminal, &f.cashier.id, "0000", NOW)
                .unwrap_err()
                .code(),
            "too_many_attempts"
        );
        assert_eq!(
            login_at(&f.db, &f.terminal, &f.cashier.id, "4826", NOW + 1_000)
                .unwrap_err()
                .code(),
            "too_many_attempts"
        );
        // Five wrong PINs, then the refused attempt on the locked user.
        assert_eq!(
            reasons(&f.db, "auth.login_failed"),
            [
                "invalid_pin",
                "invalid_pin",
                "invalid_pin",
                "invalid_pin",
                "invalid_pin",
                "locked"
            ]
        );
        assert_eq!(
            lock_state(&f.db, &f.cashier.id),
            (0, Some(NOW + LOCKOUT_MS))
        );
    }

    #[test]
    fn lockout_expires_after_five_minutes() {
        let f = fixture();
        for _ in 0..MAX_FAILED_ATTEMPTS {
            let _ = login_at(&f.db, &f.terminal, &f.owner.id, "0000", NOW);
        }
        let session = login_at(
            &f.db,
            &f.terminal,
            &f.owner.id,
            "2580",
            NOW + LOCKOUT_MS + 1,
        )
        .unwrap();
        assert!(session.is_owner);
        assert_eq!(
            login_at(
                &f.db,
                &f.terminal,
                &f.owner.id,
                "0000",
                NOW + LOCKOUT_MS + 2
            )
            .unwrap_err()
            .code(),
            "invalid_pin"
        );
    }

    #[test]
    fn a_lock_only_holds_within_one_lockout() {
        assert!(!is_locked(None, NOW));
        assert!(is_locked(Some(NOW + LOCKOUT_MS), NOW));
        assert!(is_locked(Some(NOW + 1), NOW));
        assert!(!is_locked(Some(NOW), NOW), "expired");
        assert!(
            !is_locked(Some(NOW + LOCKOUT_MS + 1), NOW),
            "dated too far ahead"
        );
        assert!(!is_locked(Some(i64::MAX), i64::MIN), "no overflow");
    }

    #[test]
    fn a_lock_dated_far_ahead_is_stale_and_cleared() {
        let f = fixture();
        // Written by a clock that ran a day ahead, then corrected.
        let far = NOW + 24 * 60 * 60 * 1_000;
        set_locked_until(&f.db, &f.cashier.id, Some(far));
        assert_eq!(
            login_at(&f.db, &f.terminal, &f.cashier.id, "0000", NOW)
                .unwrap_err()
                .code(),
            "invalid_pin"
        );
        assert_eq!(lock_state(&f.db, &f.cashier.id), (1, None));
        set_locked_until(&f.db, &f.cashier.id, Some(far));
        login_at(&f.db, &f.terminal, &f.cashier.id, "4826", NOW).unwrap();
        assert_eq!(lock_state(&f.db, &f.cashier.id), (0, None));
    }

    #[test]
    fn a_lock_set_during_verification_wins() {
        let f = fixture();
        let (id, attempt) = (f.cashier.id.clone(), sign_in(&f.terminal, NOW));
        let outcome =
            f.db.write(move |tx| {
                // Another till locked the user while this one ran Argon2 on a correct PIN.
                tx.execute(
                    "UPDATE users SET failed_attempts = 0, locked_until = ?2 WHERE id = ?1",
                    params![id, NOW + LOCKOUT_MS],
                )?;
                settle(tx, &attempt, &id, true)
            })
            .unwrap();
        assert_eq!(outcome.unwrap_err().code(), "too_many_attempts");
        assert_eq!(
            lock_state(&f.db, &f.cashier.id),
            (0, Some(NOW + LOCKOUT_MS)),
            "the lock is kept"
        );
        assert_eq!(reasons(&f.db, "auth.login_failed"), ["locked"]);

        let (id, attempt) = (f.manager.id.clone(), sign_in(&f.terminal, NOW));
        let outcome =
            f.db.write(move |tx| {
                tx.execute("UPDATE users SET active = 0 WHERE id = ?1", [&id])?;
                settle(tx, &attempt, &id, true)
            })
            .unwrap();
        assert_eq!(outcome.unwrap_err().code(), "user_inactive");
    }

    #[test]
    fn unknown_and_inactive_users_are_refused() {
        let f = fixture();
        assert_eq!(
            login_at(&f.db, &f.terminal, "nobody", "2580", NOW)
                .unwrap_err()
                .code(),
            "invalid_credentials"
        );
        assert!(
            rows(&f.db, "auth.login_failed").is_empty(),
            "no user to name"
        );
        let id = f.cashier.id.clone();
        f.db.write(move |tx| users::deactivate(tx, &Actor::default(), &id))
            .unwrap();
        assert_eq!(
            login_at(&f.db, &f.terminal, &f.cashier.id, "4826", NOW)
                .unwrap_err()
                .code(),
            "user_inactive"
        );
        let failures = rows(&f.db, "auth.login_failed");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].0.as_deref(), Some(f.cashier.id.as_str()));
        assert_eq!(failures[0].1, json!({ "reason": "inactive" }));
        assert_eq!(
            lock_state(&f.db, &f.cashier.id),
            (0, None),
            "not a wrong PIN"
        );
    }

    #[test]
    fn manager_override_records_both_users() {
        let f = fixture();
        let cashier = login_at(&f.db, &f.terminal, &f.cashier.id, "4826", NOW).unwrap();
        let grant = override_as(&f, &cashier, &f.manager.id, "7391").unwrap();
        assert_eq!(
            (grant.approver_name.as_str(), grant.permission),
            ("Karima Saadi", Permission::SalesDiscount)
        );
        let granted = rows(&f.db, "auth.override");
        assert_eq!(granted.len(), 1);
        assert_eq!(granted[0].0.as_deref(), Some(f.cashier.id.as_str()));
        assert_eq!(
            granted[0].1,
            json!({ "approverId": f.manager.id, "permission": "sales.discount" })
        );
        // The grant carries the approver's own discount limit (0 for Karima, full for an owner).
        assert_eq!(grant.discount_limit_bp, 0);
        let owner = override_as(&f, &cashier, &f.owner.id, "2580").unwrap();
        assert_eq!(owner.discount_limit_bp, 10_000);
        let manager = login_at(&f.db, &f.terminal, &f.manager.id, "7391", NOW).unwrap();
        let refused = override_as(&f, &manager, &f.cashier.id, "4826").unwrap_err();
        assert_eq!(refused.code(), "approver_lacks_permission");
        let failed = rows(&f.db, "auth.override_failed");
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].0.as_deref(), Some(f.manager.id.as_str()));
        assert_eq!(
            failed[0].1,
            json!({
                "approverId": f.cashier.id,
                "permission": "sales.discount",
                "reason": "lacks_permission",
            })
        );
    }

    /// A manager with `sales.discount` and a capped limit cannot lift their own cap.
    #[test]
    fn nobody_approves_their_own_request() {
        let f = fixture();
        let manager = login_at(&f.db, &f.terminal, &f.manager.id, "7391", NOW).unwrap();
        let refused = override_as(&f, &manager, &f.manager.id, "7391").unwrap_err();
        assert_eq!(refused.code(), "override_self_not_allowed");
        assert!(rows(&f.db, "auth.override").is_empty());
        let failed = rows(&f.db, "auth.override_failed");
        assert_eq!(failed.len(), 1);
        assert_eq!(
            failed[0].1,
            json!({
                "approverId": f.manager.id,
                "permission": "sales.discount",
                "reason": "self_approval",
            })
        );
        // By username too: the approver is resolved before the comparison.
        let refused = override_as(&f, &manager, &f.manager.username, "7391").unwrap_err();
        assert_eq!(refused.code(), "override_self_not_allowed");
    }

    #[test]
    fn wrong_approver_pins_count_toward_the_approver_lockout() {
        let f = fixture();
        let cashier = login_at(&f.db, &f.terminal, &f.cashier.id, "4826", NOW).unwrap();
        for _ in 0..4 {
            assert_eq!(
                override_as(&f, &cashier, &f.manager.id, "0000")
                    .unwrap_err()
                    .code(),
                "invalid_pin"
            );
        }
        assert_eq!(lock_state(&f.db, &f.manager.id), (4, None));
        assert_eq!(
            override_as(&f, &cashier, &f.manager.id, "0000")
                .unwrap_err()
                .code(),
            "too_many_attempts"
        );
        let failed = rows(&f.db, "auth.override_failed");
        assert_eq!(failed.len(), 5);
        assert!(failed.iter().all(|(user, payload)| {
            user.as_deref() == Some(f.cashier.id.as_str())
                && payload["approverId"] == json!(f.manager.id)
                && payload["reason"] == json!("invalid_pin")
        }));
        assert_eq!(failed[4].1["locked"], json!(true));
        // The lock is the approver's own: their next login is refused too.
        assert_eq!(
            login_at(&f.db, &f.terminal, &f.manager.id, "7391", NOW)
                .unwrap_err()
                .code(),
            "too_many_attempts"
        );
    }

    #[test]
    fn locked_or_inactive_approver_is_refused() {
        let f = fixture();
        let cashier = login_at(&f.db, &f.terminal, &f.cashier.id, "4826", NOW).unwrap();
        set_locked_until(&f.db, &f.manager.id, Some(NOW + LOCKOUT_MS));
        assert_eq!(
            override_as(&f, &cashier, &f.manager.id, "7391")
                .unwrap_err()
                .code(),
            "too_many_attempts"
        );
        set_locked_until(&f.db, &f.manager.id, None);
        let id = f.manager.id.clone();
        f.db.write(move |tx| users::deactivate(tx, &Actor::default(), &id))
            .unwrap();
        assert_eq!(
            override_as(&f, &cashier, &f.manager.id, "7391")
                .unwrap_err()
                .code(),
            "user_inactive"
        );
        assert_eq!(
            reasons(&f.db, "auth.override_failed"),
            ["locked", "inactive"]
        );
        assert_eq!(
            lock_state(&f.db, &f.manager.id),
            (0, None),
            "PIN never checked"
        );
        assert!(rows(&f.db, "auth.override").is_empty());
    }

    #[test]
    fn successful_confirm_and_override_reset_the_failure_count() {
        let f = fixture();
        let owner = login_at(&f.db, &f.terminal, &f.owner.id, "2580", NOW).unwrap();
        for _ in 0..3 {
            let _ = confirm_pin(&f.db, &f.terminal, &owner, "0000", NOW);
        }
        assert_eq!(lock_state(&f.db, &f.owner.id).0, 3);
        confirm_pin(&f.db, &f.terminal, &owner, "2580", NOW).unwrap();
        assert_eq!(lock_state(&f.db, &f.owner.id), (0, None));

        let cashier = login_at(&f.db, &f.terminal, &f.cashier.id, "4826", NOW).unwrap();
        for _ in 0..2 {
            let _ = override_as(&f, &cashier, &f.manager.id, "0000");
        }
        assert_eq!(lock_state(&f.db, &f.manager.id).0, 2);
        override_as(&f, &cashier, &f.manager.id, "7391").unwrap();
        assert_eq!(lock_state(&f.db, &f.manager.id), (0, None));
    }

    #[test]
    fn refresh_follows_the_database() {
        let f = fixture();
        let cashier = login_at(&f.db, &f.terminal, &f.cashier.id, "4826", NOW).unwrap();
        let promoted = UserUpdate {
            display_name: "Yacine B.".into(),
            role_id: MANAGER_ROLE_ID.into(),
            lang: Lang::Ar,
            discount_limit_bp: 500,
            active: true,
        };
        let (id, by_owner) = (
            f.cashier.id.clone(),
            Actor {
                user_id: Some(f.owner.id.clone()),
                terminal_id: None,
            },
        );
        f.db.write(move |tx| users::update(tx, &by_owner, &id, &promoted))
            .unwrap();
        // AUD-002: the journal shows every changed field, old → new, the discount limit too.
        assert_eq!(
            rows(&f.db, "user.updated"),
            [(
                Some(f.owner.id.clone()),
                serde_json::json!({
                    "before": {
                        "displayName": "Yacine",
                        "roleId": CASHIER_ROLE_ID,
                        "active": true,
                        "discountLimitBp": 0,
                        "lang": "fr",
                    },
                    "after": {
                        "displayName": "Yacine B.",
                        "roleId": MANAGER_ROLE_ID,
                        "active": true,
                        "discountLimitBp": 500,
                        "lang": "ar",
                    },
                })
            )]
        );
        let fresh = {
            let session = cashier.clone();
            f.db.read(move |c| refresh(c, &session)).unwrap()
        };
        assert!(fresh.has(Permission::SalesDiscount));
        assert_eq!(
            (
                fresh.display_name.as_str(),
                fresh.lang,
                fresh.discount_limit_bp
            ),
            ("Yacine B.", Lang::Ar, 500)
        );
        assert_eq!(
            (fresh.started_at, fresh.terminal_id.as_str()),
            (cashier.started_at, cashier.terminal_id.as_str()),
            "the session itself continues"
        );

        let id = f.cashier.id.clone();
        f.db.write(move |tx| users::deactivate(tx, &Actor::default(), &id))
            .unwrap();
        let session = cashier.clone();
        let gone = f.db.read(move |c| refresh(c, &session)).unwrap_err();
        assert_eq!(gone.code(), "session_invalid");
        let ghost = Session {
            user_id: "00000000-0000-7000-8000-00000000dead".into(),
            ..cashier
        };
        let missing = f.db.read(move |c| refresh(c, &ghost)).unwrap_err();
        assert_eq!(missing.code(), "session_invalid");
    }

    #[test]
    fn require_checks_the_session() {
        let f = fixture();
        let cashier = login_at(&f.db, &f.terminal, &f.cashier.id, "4826", NOW).unwrap();
        assert!(require(&cashier, Permission::SalesCreate).is_ok());
        let err = require(&cashier, Permission::SettingsUsers).unwrap_err();
        assert_eq!(err.code(), "insufficient_permission");
        assert_eq!(
            err.payload().details,
            Some(serde_json::json!({ "permission": "settings.users" }))
        );
    }

    #[test]
    fn confirm_pin_logout_and_language_are_audited() {
        let f = fixture();
        let owner = login_at(&f.db, &f.terminal, &f.owner.id, "2580", NOW).unwrap();
        assert!(confirm_pin(&f.db, &f.terminal, &owner, "2580", NOW).is_ok());
        assert_eq!(
            confirm_pin(&f.db, &f.terminal, &owner, "0000", NOW)
                .unwrap_err()
                .code(),
            "invalid_pin"
        );
        let arabic = change_language(&f.db, &owner, Lang::Ar).unwrap();
        assert_eq!(arabic.lang, Lang::Ar);
        logout(&f.db, &arabic).unwrap();
        let log = actions(&f.db);
        assert!(log.ends_with(&["user.language_changed".to_owned(), "auth.logout".to_owned()]));
        let id = f.owner.id.clone();
        assert_eq!(
            f.db.read(move |c| users::get(c, &id)).unwrap().lang,
            Lang::Ar
        );
    }
}
