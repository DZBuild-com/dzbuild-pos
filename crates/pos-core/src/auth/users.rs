//! Users: tiles for the login screen, CRUD with the "one active owner" and owner-only guards (spec §4.5).

use super::permission::Permission;
use super::pin;
use super::roles::{self, OWNER_ROLE_ID};
use crate::audit::{self, Actor};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::Rate;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::fmt;

pub const DISPLAY_NAME_MAX: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UserTile {
    pub id: String,
    pub display_name: String,
    pub role_id: String,
    pub role_name_ar: String,
    pub role_name_fr: String,
    pub role_name_en: String,
    pub lang: Lang,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role_id: String,
    pub role_name_ar: String,
    pub role_name_fr: String,
    pub role_name_en: String,
    pub is_owner: bool,
    pub lang: Lang,
    pub active: bool,
    pub discount_limit_bp: i32,
    pub last_login_at: Option<i64>,
    pub created_at: i64,
}

/// `username` is optional: it is derived from the display name when empty.
/// `pin` is hashed by the caller before the write transaction (Argon2 stays off the writer).
/// `Debug` is written by hand so that the plaintext PIN never reaches a log line.
#[derive(Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NewUser {
    pub username: Option<String>,
    pub display_name: String,
    pub pin: String,
    pub role_id: String,
    pub lang: Lang,
    pub discount_limit_bp: i32,
}

impl fmt::Debug for NewUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NewUser")
            .field("username", &self.username)
            .field("display_name", &self.display_name)
            .field("pin", &"<redacted>")
            .field("role_id", &self.role_id)
            .field("lang", &self.lang)
            .field("discount_limit_bp", &self.discount_limit_bp)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UserUpdate {
    pub display_name: String,
    pub role_id: String,
    pub lang: Lang,
    pub discount_limit_bp: i32,
    pub active: bool,
}

const USER_SELECT: &str =
    "SELECT u.id, u.username, u.display_name, u.role_id, r.name_ar, r.name_fr,
        r.name_en, u.lang, u.active, u.discount_limit_bp, u.last_login_at, u.created_at
     FROM users u JOIN roles r ON r.id = u.role_id";

fn map_user(r: &Row<'_>) -> rusqlite::Result<User> {
    let role_id: String = r.get(3)?;
    Ok(User {
        is_owner: role_id == OWNER_ROLE_ID,
        id: r.get(0)?,
        username: r.get(1)?,
        display_name: r.get(2)?,
        role_id,
        role_name_ar: r.get(4)?,
        role_name_fr: r.get(5)?,
        role_name_en: r.get(6)?,
        lang: r.get(7)?,
        active: r.get(8)?,
        discount_limit_bp: r.get(9)?,
        last_login_at: r.get(10)?,
        created_at: r.get(11)?,
    })
}

pub fn list_tiles(conn: &Connection) -> Result<Vec<UserTile>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT u.id, u.display_name, u.role_id, r.name_ar, r.name_fr, r.name_en, u.lang
         FROM users u JOIN roles r ON r.id = u.role_id
         WHERE u.active = 1 ORDER BY u.display_name COLLATE NOCASE, u.id",
    )?;
    let tiles = stmt
        .query_map([], |r| {
            Ok(UserTile {
                id: r.get(0)?,
                display_name: r.get(1)?,
                role_id: r.get(2)?,
                role_name_ar: r.get(3)?,
                role_name_fr: r.get(4)?,
                role_name_en: r.get(5)?,
                lang: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tiles)
}

pub fn list(conn: &Connection) -> Result<Vec<User>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{USER_SELECT} ORDER BY u.active DESC, u.display_name COLLATE NOCASE, u.id"
    ))?;
    let users = stmt
        .query_map([], map_user)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(users)
}

pub fn get(conn: &Connection, id: &str) -> Result<User, AppError> {
    conn.query_row(&format!("{USER_SELECT} WHERE u.id = ?1"), [id], map_user)
        .optional()?
        .ok_or_else(|| AppError::not_found("user"))
}

fn clean_display_name(raw: &str) -> Result<String, AppError> {
    let name = raw.trim().to_owned();
    if name.is_empty() {
        return Err(AppError::validation("displayName", "required"));
    }
    if name.chars().count() > DISPLAY_NAME_MAX {
        return Err(AppError::validation("displayName", "too_long"));
    }
    Ok(name)
}

fn clean_username(raw: &str) -> Result<String, AppError> {
    let username = raw.trim().to_lowercase();
    let len = username.chars().count();
    if !(2..=32).contains(&len) || username.chars().any(char::is_whitespace) {
        return Err(AppError::validation("username", "username_invalid"));
    }
    Ok(username)
}

fn username_exists(conn: &Connection, username: &str) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM users WHERE username = ?1)",
        [username],
        |r| r.get(0),
    )?)
}

/// "Amine Benali" → "amine.benali", then "amine.benali-2", … (Arabic letters kept as typed).
fn derive_username(conn: &Connection, display_name: &str) -> Result<String, AppError> {
    let joined = display_name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(".")
        .to_lowercase();
    let base: String = joined.chars().take(24).collect();
    let base = if base.chars().count() < 2 {
        "user".to_owned()
    } else {
        base
    };
    for n in 1..=999 {
        let candidate = if n == 1 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        if !username_exists(conn, &candidate)? {
            return Ok(candidate);
        }
    }
    Err(AppError::conflict("username_taken"))
}

fn check_discount(bp: i32) -> Result<(), AppError> {
    if Rate(bp).is_fraction() {
        Ok(())
    } else {
        Err(AppError::validation(
            "discountLimitBp",
            "discount_limit_invalid",
        ))
    }
}

/// Only Argon2id PHC strings are stored: a plaintext PIN reaching this point is a caller bug.
fn check_pin_hash(pin_hash: &str) -> Result<(), AppError> {
    if pin::is_pin_hash(pin_hash) {
        Ok(())
    } else {
        Err(AppError::Internal(
            "PIN hash is not an Argon2id PHC string".into(),
        ))
    }
}

fn active_owners(conn: &Connection) -> Result<i64, AppError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM users WHERE active = 1 AND role_id = ?1",
        [OWNER_ROLE_ID],
        |r| r.get(0),
    )?)
}

/// The first active owner, oldest first: who a till's own background calls to the main PC run as
/// when nobody is signed in (a read that needs no right, final review).
pub fn first_active_owner(conn: &Connection) -> Result<Option<String>, AppError> {
    Ok(conn
        .query_row(
            "SELECT id FROM users WHERE role_id = ?1 AND active = 1 ORDER BY created_at, id LIMIT 1",
            [OWNER_ROLE_ID],
            |r| r.get(0),
        )
        .optional()?)
}

/// The shop must always keep at least one active owner.
pub fn ensure_owner_remains(conn: &Connection) -> Result<(), AppError> {
    if active_owners(conn)? == 0 {
        return Err(AppError::conflict("last_owner"));
    }
    Ok(())
}

/// Owner accounts and the owner role are managed by owners only (spec §4.5): a manager with
/// `settings.users` cannot create an owner, promote anyone to owner, or edit, re-PIN,
/// deactivate or reactivate an owner. The system actor (no user) passes only while no active
/// owner exists yet, that is during the first-run setup; after that it is refused like anyone
/// who is not an active owner.
fn ensure_actor_is_owner(conn: &Connection, actor: &Actor) -> Result<(), AppError> {
    let allowed = match actor.user_id.as_deref() {
        None => active_owners(conn)? == 0,
        Some(actor_id) => conn
            .query_row(
                "SELECT role_id = ?2 AND active = 1 FROM users WHERE id = ?1",
                params![actor_id, OWNER_ROLE_ID],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(false),
    };
    if allowed {
        Ok(())
    } else {
        Err(AppError::auth("owner_only"))
    }
}

/// Whether `actor` is an active user whose role grants `permission`; never the system actor.
fn actor_has(conn: &Connection, actor: &Actor, permission: Permission) -> Result<bool, AppError> {
    let Some(actor_id) = actor.user_id.as_deref() else {
        return Ok(false);
    };
    let role_id: Option<String> = conn
        .query_row(
            "SELECT role_id FROM users WHERE id = ?1 AND active = 1",
            [actor_id],
            |r| r.get(0),
        )
        .optional()?;
    match role_id {
        Some(role_id) => Ok(roles::permissions_of(conn, &role_id)?.contains(&permission)),
        None => Ok(false),
    }
}

pub fn create(
    tx: &Transaction<'_>,
    actor: &Actor,
    input: &NewUser,
    pin_hash: &str,
) -> Result<User, AppError> {
    insert(tx, actor, input, pin_hash, None)
}

/// USR-015: a new user on the model of `template_id` — its role, language and discount limit —
/// with their own name and PIN (hashed by the caller). The Propriétaire stays unique: an owner
/// is never a template. Journaled once, as `user.duplicated` with its template.
pub fn duplicate(
    tx: &Transaction<'_>,
    actor: &Actor,
    template_id: &str,
    display_name: &str,
    pin_hash: &str,
) -> Result<User, AppError> {
    let template = get(tx, template_id)?;
    if template.is_owner {
        return Err(AppError::validation("templateId", "value_invalid"));
    }
    let input = NewUser {
        username: None,
        display_name: display_name.to_owned(),
        pin: String::new(),
        role_id: template.role_id,
        lang: template.lang,
        discount_limit_bp: template.discount_limit_bp,
    };
    insert(tx, actor, &input, pin_hash, Some(template_id))
}

/// A new user, journaled as `user.created`, or as `user.duplicated` when made `from` a template.
fn insert(
    tx: &Transaction<'_>,
    actor: &Actor,
    input: &NewUser,
    pin_hash: &str,
    from: Option<&str>,
) -> Result<User, AppError> {
    check_pin_hash(pin_hash)?;
    let display_name = clean_display_name(&input.display_name)?;
    check_discount(input.discount_limit_bp)?;
    roles::get(tx, &input.role_id)?;
    if input.role_id == OWNER_ROLE_ID {
        ensure_actor_is_owner(tx, actor)?;
    }
    let explicit = input
        .username
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty());
    let username = match explicit {
        Some(raw) => {
            let username = clean_username(raw)?;
            if username_exists(tx, &username)? {
                return Err(AppError::conflict("username_taken"));
            }
            username
        }
        None => derive_username(tx, &display_name)?,
    };
    let id = new_id();
    let now = now_ms();
    tx.execute(
        "INSERT INTO users (id, username, display_name, pin_hash, role_id, lang, active, discount_limit_bp,
                            created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?8)",
        params![id, username, display_name, pin_hash, input.role_id, input.lang, input.discount_limit_bp, now],
    )?;
    let mut payload = json!({
        "username": username,
        "displayName": display_name,
        "roleId": input.role_id,
        "discountLimitBp": input.discount_limit_bp,
        "lang": input.lang,
    });
    let action = match from {
        Some(template) => {
            payload["from"] = json!(template);
            "user.duplicated"
        }
        None => "user.created",
    };
    audit::append(tx, &actor.event(action, "user", Some(&id), payload))?;
    get(tx, &id)
}

/// The fields a user edit can change, as the journal records them before and after (AUD-002).
fn journal_snapshot(
    display_name: &str,
    role_id: &str,
    active: bool,
    discount_limit_bp: i32,
    lang: Lang,
) -> serde_json::Value {
    json!({
        "displayName": display_name,
        "roleId": role_id,
        "active": active,
        "discountLimitBp": discount_limit_bp,
        "lang": lang,
    })
}

pub fn update(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    input: &UserUpdate,
) -> Result<User, AppError> {
    let before = get(tx, id)?;
    if before.is_owner || input.role_id == OWNER_ROLE_ID {
        ensure_actor_is_owner(tx, actor)?;
    }
    let display_name = clean_display_name(&input.display_name)?;
    check_discount(input.discount_limit_bp)?;
    roles::get(tx, &input.role_id)?;
    if !input.active && actor.user_id.as_deref() == Some(id) {
        return Err(AppError::conflict("cannot_deactivate_self"));
    }
    tx.execute(
        "UPDATE users SET display_name = ?2, role_id = ?3, lang = ?4, discount_limit_bp = ?5, active = ?6,
                          updated_at = ?7
         WHERE id = ?1",
        params![id, display_name, input.role_id, input.lang, input.discount_limit_bp, input.active, now_ms()],
    )?;
    ensure_owner_remains(tx)?;
    audit::append(
        tx,
        &actor.event(
            "user.updated",
            "user",
            Some(id),
            json!({
                "before": journal_snapshot(
                    &before.display_name,
                    &before.role_id,
                    before.active,
                    before.discount_limit_bp,
                    before.lang,
                ),
                "after": journal_snapshot(
                    &display_name,
                    &input.role_id,
                    input.active,
                    input.discount_limit_bp,
                    input.lang,
                ),
            }),
        ),
    )?;
    get(tx, id)
}

pub fn set_pin_hash(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    pin_hash: &str,
) -> Result<(), AppError> {
    check_pin_hash(pin_hash)?;
    if get(tx, id)?.is_owner {
        ensure_actor_is_owner(tx, actor)?;
    }
    tx.execute(
        "UPDATE users SET pin_hash = ?2, failed_attempts = 0, locked_until = NULL, updated_at = ?3 WHERE id = ?1",
        params![id, pin_hash, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event("user.pin_changed", "user", Some(id), json!({})),
    )?;
    Ok(())
}

pub fn deactivate(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    if actor.user_id.as_deref() == Some(id) {
        return Err(AppError::conflict("cannot_deactivate_self"));
    }
    if get(tx, id)?.is_owner {
        ensure_actor_is_owner(tx, actor)?;
    }
    tx.execute(
        "UPDATE users SET active = 0, updated_at = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    ensure_owner_remains(tx)?;
    audit::append(
        tx,
        &actor.event("user.deactivated", "user", Some(id), json!({})),
    )?;
    Ok(())
}

/// Changes a user's language: users change their own, a `settings.users` holder anyone's.
pub fn set_language(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    lang: Lang,
) -> Result<(), AppError> {
    if actor.user_id.as_deref() != Some(id) && !actor_has(tx, actor, Permission::SettingsUsers)? {
        return Err(AppError::Permission {
            permission: Permission::SettingsUsers.as_str().to_owned(),
        });
    }
    let changed = tx.execute(
        "UPDATE users SET lang = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, lang, now_ms()],
    )?;
    if changed == 0 {
        return Err(AppError::not_found("user"));
    }
    audit::append(
        tx,
        &actor.event(
            "user.language_changed",
            "user",
            Some(id),
            json!({ "lang": lang }),
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::roles::{self, CASHIER_ROLE_ID, MANAGER_ROLE_ID, OWNER_ROLE_ID};
    use crate::db::Db;

    /// Well-formed Argon2id PHC strings: the store checks the format, never the PIN.
    const HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$ZmFrZWhhc2hmYWtlaGFzaGZha2VoYXNoZmFrZWhhc2g";
    const HASH2: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$b3RoZXJoYXNob3RoZXJoYXNob3RoZXJoYXNob3RoZXI";

    fn seeded() -> Db {
        let db = Db::open_memory().unwrap();
        db.write(|tx| roles::seed_system_roles(tx)).unwrap();
        db
    }

    fn new_user(name: &str, role: &str, username: Option<&str>) -> NewUser {
        NewUser {
            username: username.map(str::to_owned),
            display_name: name.into(),
            pin: String::new(),
            role_id: role.into(),
            lang: Lang::Fr,
            discount_limit_bp: 0,
        }
    }

    /// Creates as the system actor: any user before the first owner, non-owners after it.
    fn add(db: &Db, input: NewUser) -> Result<User, AppError> {
        add_as(db, &Actor::default(), input)
    }

    fn add_as(db: &Db, actor: &Actor, input: NewUser) -> Result<User, AppError> {
        let actor = actor.clone();
        db.write(move |tx| create(tx, &actor, &input, HASH))
    }

    fn as_user(user: &User) -> Actor {
        Actor {
            user_id: Some(user.id.clone()),
            terminal_id: None,
        }
    }

    fn update_of(user: &User) -> UserUpdate {
        UserUpdate {
            display_name: user.display_name.clone(),
            role_id: user.role_id.clone(),
            lang: user.lang,
            discount_limit_bp: user.discount_limit_bp,
            active: user.active,
        }
    }

    /// Payload of every audit row with `action`, oldest first.
    fn payloads(db: &Db, action: &'static str) -> Vec<serde_json::Value> {
        db.read(move |c| {
            let mut s =
                c.prepare("SELECT payload FROM audit_log WHERE action = ?1 ORDER BY seq")?;
            let v = s
                .query_map([action], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
        .unwrap()
        .iter()
        .map(|p| serde_json::from_str(p).unwrap())
        .collect()
    }

    fn stored_hash(db: &Db, id: &str) -> String {
        let id = id.to_owned();
        db.read(move |c| {
            Ok(
                c.query_row("SELECT pin_hash FROM users WHERE id = ?1", [&id], |r| {
                    r.get(0)
                })?,
            )
        })
        .unwrap()
    }

    #[test]
    fn usernames_are_derived_and_unique() {
        let db = seeded();
        let first = add(&db, new_user("Amine Benali", CASHIER_ROLE_ID, None)).unwrap();
        let second = add(&db, new_user("Amine  Benali", CASHIER_ROLE_ID, None)).unwrap();
        assert_eq!(
            (first.username.as_str(), second.username.as_str()),
            ("amine.benali", "amine.benali-2")
        );
        let arabic = add(&db, new_user("ياسين", CASHIER_ROLE_ID, None)).unwrap();
        assert_eq!(arabic.username, "ياسين");
    }

    #[test]
    fn explicit_usernames_are_checked() {
        let db = seeded();
        add(&db, new_user("Caisse", CASHIER_ROLE_ID, Some("Caisse1"))).unwrap();
        let taken = add(&db, new_user("Autre", CASHIER_ROLE_ID, Some("caisse1"))).unwrap_err();
        assert_eq!(taken.code(), "username_taken");
        let too_long = "x".repeat(33);
        for bad in ["a", "a b", too_long.as_str()] {
            assert_eq!(
                add(&db, new_user("X", CASHIER_ROLE_ID, Some(bad)))
                    .unwrap_err()
                    .code(),
                "username_invalid"
            );
        }
    }

    #[test]
    fn fields_are_validated() {
        let db = seeded();
        assert_eq!(
            add(&db, new_user("  ", CASHIER_ROLE_ID, None))
                .unwrap_err()
                .code(),
            "required"
        );
        let mut greedy = new_user("Samir", CASHIER_ROLE_ID, None);
        greedy.discount_limit_bp = 10_001;
        assert_eq!(
            add(&db, greedy).unwrap_err().code(),
            "discount_limit_invalid"
        );
        let ghost = new_user("Samir", "00000000-0000-7000-8000-00000000ffff", None);
        assert_eq!(add(&db, ghost).unwrap_err().code(), "role_not_found");
    }

    #[test]
    fn ensure_owner_remains_needs_an_active_owner() {
        let db = seeded();
        assert_eq!(
            db.read(ensure_owner_remains).unwrap_err().code(),
            "last_owner"
        );
        add(&db, new_user("Amine", OWNER_ROLE_ID, None)).unwrap();
        db.read(ensure_owner_remains).unwrap();
    }

    #[test]
    fn last_owner_cannot_be_deactivated_or_demoted() {
        let db = seeded();
        let owner = add(&db, new_user("Amine", OWNER_ROLE_ID, None)).unwrap();
        // Once an owner exists the system actor is no longer trusted with owners, the owner
        // cannot deactivate themself, and no other owner exists: every route is closed.
        let id = owner.id.clone();
        assert_eq!(
            db.write(move |tx| deactivate(tx, &Actor::default(), &id))
                .unwrap_err()
                .code(),
            "owner_only"
        );
        let mut demoted = update_of(&owner);
        demoted.role_id = CASHIER_ROLE_ID.into();
        let id = owner.id.clone();
        let by_self = as_user(&owner);
        assert_eq!(
            db.write(move |tx| update(tx, &by_self, &id, &demoted))
                .unwrap_err()
                .code(),
            "last_owner"
        );
        let nadia = add_as(
            &db,
            &as_user(&owner),
            new_user("Nadia", OWNER_ROLE_ID, None),
        )
        .unwrap();
        let id = owner.id.clone();
        let by_nadia = as_user(&nadia);
        db.write(move |tx| deactivate(tx, &by_nadia, &id)).unwrap();
        assert!(!db.read(move |c| get(c, &owner.id)).unwrap().active);
    }

    #[test]
    fn an_inactive_owner_does_not_count_as_the_remaining_owner() {
        let db = seeded();
        let amine = add(&db, new_user("Amine", OWNER_ROLE_ID, None)).unwrap();
        let by_amine = as_user(&amine);
        let nadia = add_as(&db, &by_amine, new_user("Nadia", OWNER_ROLE_ID, None)).unwrap();
        let (id, actor) = (nadia.id.clone(), by_amine.clone());
        db.write(move |tx| deactivate(tx, &actor, &id)).unwrap();
        let mut demoted = update_of(&amine);
        demoted.role_id = MANAGER_ROLE_ID.into();
        let id = amine.id.clone();
        assert_eq!(
            db.write(move |tx| update(tx, &by_amine, &id, &demoted))
                .unwrap_err()
                .code(),
            "last_owner"
        );
        assert!(db.read(move |c| get(c, &amine.id)).unwrap().is_owner);
    }

    #[test]
    fn system_actor_is_trusted_with_owners_only_before_the_first_one() {
        let db = seeded();
        let first = add(&db, new_user("Amine", OWNER_ROLE_ID, None)).unwrap();
        assert!(first.is_owner, "first-run setup creates the first owner");
        let system = Actor::system(None);
        assert_eq!(
            add_as(&db, &system, new_user("Intrus", OWNER_ROLE_ID, None))
                .unwrap_err()
                .code(),
            "owner_only"
        );
        let manager = add_as(&db, &system, new_user("Karima", MANAGER_ROLE_ID, None)).unwrap();
        let mut promoted = update_of(&manager);
        promoted.role_id = OWNER_ROLE_ID.into();
        let (id, actor) = (manager.id.clone(), system.clone());
        assert_eq!(
            db.write(move |tx| update(tx, &actor, &id, &promoted))
                .unwrap_err()
                .code(),
            "owner_only"
        );
        let (id, actor) = (first.id.clone(), system.clone());
        assert_eq!(
            db.write(move |tx| set_pin_hash(tx, &actor, &id, HASH2))
                .unwrap_err()
                .code(),
            "owner_only"
        );
        assert_eq!(stored_hash(&db, &first.id), HASH);
        let owners = db
            .read(list)
            .unwrap()
            .into_iter()
            .filter(|u| u.is_owner)
            .count();
        assert_eq!(owners, 1);
    }

    #[test]
    fn manager_cannot_create_or_promote_owner() {
        let db = seeded();
        add(&db, new_user("Amine", OWNER_ROLE_ID, None)).unwrap();
        let manager = add(&db, new_user("Karima", MANAGER_ROLE_ID, None)).unwrap();
        let by_manager = as_user(&manager);
        let created = add_as(&db, &by_manager, new_user("Intrus", OWNER_ROLE_ID, None));
        assert_eq!(created.unwrap_err().code(), "owner_only");
        let mut promoted = update_of(&manager);
        promoted.role_id = OWNER_ROLE_ID.into();
        let id = manager.id.clone();
        assert_eq!(
            db.write(move |tx| update(tx, &by_manager, &id, &promoted))
                .unwrap_err()
                .code(),
            "owner_only"
        );
        assert_eq!(
            db.read(move |c| get(c, &manager.id)).unwrap().role_id,
            MANAGER_ROLE_ID
        );
    }

    #[test]
    fn manager_cannot_reset_owner_pin() {
        let db = seeded();
        let owner = add(&db, new_user("Amine", OWNER_ROLE_ID, None)).unwrap();
        add_as(
            &db,
            &as_user(&owner),
            new_user("Nadia", OWNER_ROLE_ID, None),
        )
        .unwrap();
        let manager = add(&db, new_user("Karima", MANAGER_ROLE_ID, None)).unwrap();
        let by_manager = as_user(&manager);
        let (id, actor) = (owner.id.clone(), by_manager.clone());
        assert_eq!(
            db.write(move |tx| set_pin_hash(tx, &actor, &id, HASH2))
                .unwrap_err()
                .code(),
            "owner_only"
        );
        let (id, actor) = (owner.id.clone(), by_manager.clone());
        assert_eq!(
            db.write(move |tx| deactivate(tx, &actor, &id))
                .unwrap_err()
                .code(),
            "owner_only"
        );
        let mut renamed = update_of(&owner);
        renamed.display_name = "Autre".into();
        let id = owner.id.clone();
        assert_eq!(
            db.write(move |tx| update(tx, &by_manager, &id, &renamed))
                .unwrap_err()
                .code(),
            "owner_only"
        );
        assert_eq!(
            stored_hash(&db, &owner.id),
            HASH,
            "the owner's PIN is unchanged"
        );
        assert!(db.read(move |c| get(c, &owner.id)).unwrap().active);
    }

    #[test]
    fn manager_cannot_reactivate_an_inactive_owner() {
        let db = seeded();
        let amine = add(&db, new_user("Amine", OWNER_ROLE_ID, None)).unwrap();
        let by_amine = as_user(&amine);
        let nadia = add_as(&db, &by_amine, new_user("Nadia", OWNER_ROLE_ID, None)).unwrap();
        let manager = add(&db, new_user("Karima", MANAGER_ROLE_ID, None)).unwrap();
        let id = nadia.id.clone();
        db.write(move |tx| deactivate(tx, &by_amine, &id)).unwrap();
        let mut back = update_of(&nadia);
        back.active = true;
        let (id, by_manager) = (nadia.id.clone(), as_user(&manager));
        assert_eq!(
            db.write(move |tx| update(tx, &by_manager, &id, &back))
                .unwrap_err()
                .code(),
            "owner_only"
        );
        assert!(!db.read(move |c| get(c, &nadia.id)).unwrap().active);
    }

    #[test]
    fn owner_can_edit_owner() {
        let db = seeded();
        let amine = add(&db, new_user("Amine", OWNER_ROLE_ID, None)).unwrap();
        let by_amine = as_user(&amine);
        let nadia = add_as(&db, &by_amine, new_user("Nadia", OWNER_ROLE_ID, None)).unwrap();
        let (id, actor) = (nadia.id.clone(), by_amine.clone());
        db.write(move |tx| set_pin_hash(tx, &actor, &id, HASH2))
            .unwrap();
        assert_eq!(stored_hash(&db, &nadia.id), HASH2);
        let mut renamed = update_of(&nadia);
        renamed.display_name = "Nadia B.".into();
        let (id, actor) = (nadia.id.clone(), by_amine.clone());
        assert_eq!(
            db.write(move |tx| update(tx, &actor, &id, &renamed))
                .unwrap()
                .display_name,
            "Nadia B."
        );
        let mut limits = update_of(&nadia);
        limits.display_name = "Nadia Benali".into();
        limits.discount_limit_bp = 10_000;
        limits.lang = Lang::En;
        let (id, actor) = (nadia.id.clone(), by_amine.clone());
        db.write(move |tx| update(tx, &actor, &id, &limits))
            .unwrap();
        // AUD-002/AUD-007: a raised discount limit shows in the journal, old → new, with the
        // name the user had before.
        let updated = payloads(&db, "user.updated");
        assert_eq!(
            updated.last().unwrap(),
            &json!({
                "before": {
                    "displayName": "Nadia B.",
                    "roleId": OWNER_ROLE_ID,
                    "active": true,
                    "discountLimitBp": 0,
                    "lang": "fr",
                },
                "after": {
                    "displayName": "Nadia Benali",
                    "roleId": OWNER_ROLE_ID,
                    "active": true,
                    "discountLimitBp": 10_000,
                    "lang": "en",
                },
            })
        );
        let mut samir = new_user("Samir", OWNER_ROLE_ID, None);
        samir.discount_limit_bp = 1_500;
        samir.lang = Lang::Ar;
        let third = add_as(&db, &by_amine, samir).unwrap();
        assert!(third.is_owner);
        assert_eq!(
            payloads(&db, "user.created").last().unwrap(),
            &json!({
                "username": "samir",
                "displayName": "Samir",
                "roleId": OWNER_ROLE_ID,
                "discountLimitBp": 1_500,
                "lang": "ar",
            })
        );
        let id = nadia.id.clone();
        db.write(move |tx| deactivate(tx, &by_amine, &id)).unwrap();
        assert!(!db.read(move |c| get(c, &nadia.id)).unwrap().active);
    }

    #[test]
    fn cannot_deactivate_self() {
        let db = seeded();
        let owner = add(&db, new_user("Amine", OWNER_ROLE_ID, None)).unwrap();
        add_as(
            &db,
            &as_user(&owner),
            new_user("Nadia", OWNER_ROLE_ID, None),
        )
        .unwrap();
        let actor = as_user(&owner);
        let id = owner.id.clone();
        assert_eq!(
            db.write(move |tx| deactivate(tx, &actor, &id))
                .unwrap_err()
                .code(),
            "cannot_deactivate_self"
        );
        let mut off = update_of(&owner);
        off.active = false;
        let actor = as_user(&owner);
        assert_eq!(
            db.write(move |tx| update(tx, &actor, &owner.id, &off))
                .unwrap_err()
                .code(),
            "cannot_deactivate_self"
        );
    }

    #[test]
    fn set_pin_resets_the_lockout_and_is_audited() {
        let db = seeded();
        let user = add(&db, new_user("Yacine", CASHIER_ROLE_ID, None)).unwrap();
        let id = user.id.clone();
        db.write(move |tx| {
            tx.execute(
                "UPDATE users SET failed_attempts = 4, locked_until = 9999999999999 WHERE id = ?1",
                [&id],
            )?;
            Ok(())
        })
        .unwrap();
        let id = user.id.clone();
        db.write(move |tx| set_pin_hash(tx, &Actor::default(), &id, HASH2))
            .unwrap();
        let id = user.id.clone();
        let (attempts, locked, hash): (i64, Option<i64>, String) = db
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT failed_attempts, locked_until, pin_hash FROM users WHERE id = ?1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?)
            })
            .unwrap();
        assert_eq!((attempts, locked, hash.as_str()), (0, None, HASH2));
        let actions: i64 = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action = 'user.pin_changed'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(actions, 1);
    }

    #[test]
    fn only_argon2id_hashes_are_stored() {
        let db = seeded();
        let plaintext = db.write(|tx| {
            create(
                tx,
                &Actor::default(),
                &new_user("Yacine", CASHIER_ROLE_ID, None),
                "2580",
            )
        });
        assert_eq!(plaintext.unwrap_err().code(), "internal_error");
        let user = add(&db, new_user("Yacine", CASHIER_ROLE_ID, None)).unwrap();
        let id = user.id.clone();
        let placeholder = db.write(move |tx| set_pin_hash(tx, &Actor::default(), &id, "hash"));
        assert_eq!(placeholder.unwrap_err().code(), "internal_error");
        assert_eq!(stored_hash(&db, &user.id), HASH);
    }

    #[test]
    fn new_user_debug_hides_the_pin() {
        let mut input = new_user("Yacine", CASHIER_ROLE_ID, None);
        input.pin = "739184".into();
        let printed = format!("{input:?}");
        assert!(
            !printed.contains("739184") && printed.contains("<redacted>"),
            "{printed}"
        );
        assert!(printed.contains("Yacine"), "{printed}");
    }

    #[test]
    fn language_is_changed_by_the_user_or_a_user_manager() {
        let db = seeded();
        let owner = add(&db, new_user("Amine", OWNER_ROLE_ID, None)).unwrap();
        let manager = add(&db, new_user("Karima", MANAGER_ROLE_ID, None)).unwrap();
        let cashier = add(&db, new_user("Yacine", CASHIER_ROLE_ID, None)).unwrap();
        let change = |actor: Actor, id: &str, lang: Lang| {
            let id = id.to_owned();
            db.write(move |tx| set_language(tx, &actor, &id, lang))
        };
        change(as_user(&cashier), &cashier.id, Lang::Ar).unwrap();
        change(as_user(&owner), &cashier.id, Lang::En).unwrap();
        for (actor, target) in [
            (as_user(&cashier), &manager.id),
            (as_user(&manager), &cashier.id),
            (Actor::default(), &cashier.id),
        ] {
            let err = change(actor, target, Lang::Ar).unwrap_err();
            assert_eq!(err.code(), "insufficient_permission");
            assert_eq!(
                err.payload().details,
                Some(json!({ "permission": "settings.users" }))
            );
        }
        let id = cashier.id.clone();
        assert_eq!(db.read(move |c| get(c, &id)).unwrap().lang, Lang::En);
        let changes: i64 = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action = 'user.language_changed'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(changes, 2);
    }

    #[test]
    fn tiles_show_only_active_users() {
        let db = seeded();
        add(&db, new_user("Amine", OWNER_ROLE_ID, None)).unwrap();
        let gone = add(&db, new_user("Bilal", CASHIER_ROLE_ID, None)).unwrap();
        let id = gone.id.clone();
        db.write(move |tx| deactivate(tx, &Actor::default(), &id))
            .unwrap();
        let tiles = db.read(list_tiles).unwrap();
        assert_eq!(
            tiles
                .iter()
                .map(|t| t.display_name.as_str())
                .collect::<Vec<_>>(),
            ["Amine"]
        );
        assert_eq!(tiles[0].role_name_fr, "Propriétaire");
        assert_eq!(db.read(list).unwrap().len(), 2);
    }
}
