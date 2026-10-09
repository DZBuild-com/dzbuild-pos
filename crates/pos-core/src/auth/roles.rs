//! Built-in and custom roles (spec §4.5).

use super::permission::{Permission, PermissionGroup};
use crate::audit::{self, Actor};
use crate::error::AppError;
use crate::ids::new_id;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use specta::Type;

pub const OWNER_ROLE_ID: &str = "00000000-0000-7000-8000-000000000001";
pub const MANAGER_ROLE_ID: &str = "00000000-0000-7000-8000-000000000002";
pub const CASHIER_ROLE_ID: &str = "00000000-0000-7000-8000-000000000003";
pub const STOREKEEPER_ROLE_ID: &str = "00000000-0000-7000-8000-000000000004";
pub const WAITER_ROLE_ID: &str = "00000000-0000-7000-8000-000000000005";
pub const TECHNICIAN_ROLE_ID: &str = "00000000-0000-7000-8000-000000000006";
pub const ACCOUNTANT_ROLE_ID: &str = "00000000-0000-7000-8000-000000000007";
pub const EMPLOYEE_ROLE_ID: &str = "00000000-0000-7000-8000-000000000008";

const NAME_MAX: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Role {
    pub id: String,
    pub code: Option<String>,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub is_system: bool,
    pub is_owner: bool,
    pub permissions: Vec<Permission>,
    /// Every user holding the role, active or deactivated (users are never deleted).
    pub user_count: i64,
    /// Only the active ones.
    pub active_user_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoleInput {
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub permissions: Vec<Permission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PermissionGroupInfo {
    pub group: PermissionGroup,
    pub permissions: Vec<Permission>,
}

struct SystemRole {
    id: &'static str,
    code: &'static str,
    names: [&'static str; 3],
    permissions: fn() -> Vec<Permission>,
}

const SYSTEM_ROLES: [SystemRole; 8] = [
    SystemRole {
        id: OWNER_ROLE_ID,
        code: "owner",
        names: ["المالك", "Propriétaire", "Owner"],
        permissions: owner_permissions,
    },
    SystemRole {
        id: MANAGER_ROLE_ID,
        code: "manager",
        names: ["المسيّر", "Gérant", "Manager"],
        permissions: manager_permissions,
    },
    SystemRole {
        id: CASHIER_ROLE_ID,
        code: "cashier",
        names: ["أمين الصندوق", "Caissier", "Cashier"],
        permissions: cashier_permissions,
    },
    SystemRole {
        id: STOREKEEPER_ROLE_ID,
        code: "storekeeper",
        names: ["أمين المخزن", "Magasinier", "Storekeeper"],
        permissions: storekeeper_permissions,
    },
    // M8 trade roles (USR-003).
    SystemRole {
        id: WAITER_ROLE_ID,
        code: "waiter",
        names: ["النادل", "Serveur", "Waiter"],
        permissions: waiter_permissions,
    },
    SystemRole {
        id: TECHNICIAN_ROLE_ID,
        code: "technician",
        names: ["التقني", "Technicien", "Technician"],
        permissions: technician_permissions,
    },
    SystemRole {
        id: ACCOUNTANT_ROLE_ID,
        code: "accountant",
        names: ["المحاسب", "Comptable", "Accountant"],
        permissions: accountant_permissions,
    },
    SystemRole {
        id: EMPLOYEE_ROLE_ID,
        code: "employee",
        names: ["الموظف", "Employé", "Employee"],
        permissions: employee_permissions,
    },
];

fn owner_permissions() -> Vec<Permission> {
    Permission::ALL.to_vec()
}

fn manager_permissions() -> Vec<Permission> {
    Permission::ALL
        .into_iter()
        .filter(|p| {
            !matches!(
                p,
                Permission::SettingsCompany
                    | Permission::SettingsUsers
                    | Permission::SettingsData
                    | Permission::SettingsDzbuild
            )
        })
        .collect()
}

pub fn cashier_permissions() -> Vec<Permission> {
    vec![
        Permission::SalesCreate,
        Permission::SalesHold,
        Permission::SalesReprint,
        Permission::CashOpen,
        Permission::CashClose,
        Permission::CashPaidInOut,
        Permission::ProductsView,
        Permission::StockView,
        Permission::CustomersView,
        Permission::CreditView,
        Permission::CreditCollect,
    ]
}

fn storekeeper_permissions() -> Vec<Permission> {
    vec![
        Permission::ProductsView,
        Permission::ProductsEdit,
        Permission::ProductsImport,
        Permission::ProductsLabels,
        Permission::StockView,
        Permission::StockAdjust,
        Permission::StockCount,
        Permission::StockTransfer,
        Permission::PurchasesView,
        Permission::PurchasesManage,
        Permission::SuppliersManage,
        Permission::ReportsStock,
    ]
}

fn waiter_permissions() -> Vec<Permission> {
    vec![
        Permission::SalesCreate,
        Permission::SalesHold,
        Permission::SalesReprint,
        Permission::ProductsView,
        Permission::CustomersView,
    ]
}

fn technician_permissions() -> Vec<Permission> {
    vec![
        Permission::SalesCreate,
        Permission::SalesReprint,
        Permission::ProductsView,
        Permission::StockView,
        Permission::CustomersView,
    ]
}

fn accountant_permissions() -> Vec<Permission> {
    vec![
        Permission::SalesViewAll,
        Permission::CashViewHistory,
        Permission::PurchasesView,
        Permission::CustomersView,
        Permission::CreditView,
        Permission::ReportsSales,
        Permission::ReportsStock,
        Permission::ReportsProfit,
        Permission::ReportsCash,
        Permission::ReportsTax,
        Permission::AuditView,
    ]
}

fn employee_permissions() -> Vec<Permission> {
    vec![
        Permission::SalesCreate,
        Permission::SalesHold,
        Permission::ProductsView,
        Permission::StockView,
        Permission::CustomersView,
    ]
}

/// Inserts the built-in roles when missing; never overwrites an owner's edits (an existing shop
/// gets the M8 trade roles at its next start).
pub fn seed_system_roles(tx: &Transaction<'_>) -> Result<(), AppError> {
    let now = now_ms();
    for role in &SYSTEM_ROLES {
        let permissions = serde_json::to_string(&(role.permissions)())?;
        tx.execute(
            "INSERT INTO roles (id, code, name_ar, name_fr, name_en, is_system, permissions, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, ?7)
             ON CONFLICT (id) DO NOTHING",
            params![role.id, role.code, role.names[0], role.names[1], role.names[2], permissions, now],
        )?;
    }
    Ok(())
}

const ROLE_SELECT: &str =
    "SELECT r.id, r.code, r.name_ar, r.name_fr, r.name_en, r.is_system, r.permissions,
        (SELECT COUNT(*) FROM users u WHERE u.role_id = r.id),
        (SELECT COUNT(*) FROM users u WHERE u.role_id = r.id AND u.active = 1)
     FROM roles r";

/// Stored permissions of a role, failing closed: an unknown name is dropped and a list that is
/// not all strings grants nothing. Both are logged so that a damaged role can be found.
fn parse_permissions(role_id: &str, stored: &str) -> Vec<Permission> {
    let names: Vec<String> = match serde_json::from_str(stored) {
        Ok(names) => names,
        Err(e) => {
            tracing::warn!(role_id, error = %e, "unreadable role permissions; granting none");
            return Vec::new();
        }
    };
    let mut out = Vec::with_capacity(names.len());
    for name in &names {
        match Permission::parse(name) {
            Some(permission) => out.push(permission),
            None => tracing::warn!(role_id, permission = %name, "unknown role permission dropped"),
        }
    }
    normalize_permissions(&mut out);
    out
}

fn normalize_permissions(list: &mut Vec<Permission>) {
    list.sort_by_key(|p| p.index());
    list.dedup();
}

fn map_role(row: &Row<'_>) -> rusqlite::Result<Role> {
    let id: String = row.get(0)?;
    let stored: String = row.get(6)?;
    let is_owner = id == OWNER_ROLE_ID;
    Ok(Role {
        permissions: if is_owner {
            Permission::ALL.to_vec()
        } else {
            parse_permissions(&id, &stored)
        },
        is_owner,
        id,
        code: row.get(1)?,
        name_ar: row.get(2)?,
        name_fr: row.get(3)?,
        name_en: row.get(4)?,
        is_system: row.get(5)?,
        user_count: row.get(7)?,
        active_user_count: row.get(8)?,
    })
}

/// Active roles: built-in first, then custom roles by creation.
pub fn list(conn: &Connection) -> Result<Vec<Role>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{ROLE_SELECT} WHERE r.archived_at IS NULL ORDER BY r.is_system DESC, r.created_at, r.id"
    ))?;
    let roles = stmt
        .query_map([], map_role)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(roles)
}

pub fn get(conn: &Connection, id: &str) -> Result<Role, AppError> {
    conn.query_row(
        &format!("{ROLE_SELECT} WHERE r.id = ?1 AND r.archived_at IS NULL"),
        [id],
        map_role,
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("role"))
}

/// Effective permissions of a role (the owner always has all of them).
pub fn permissions_of(conn: &Connection, role_id: &str) -> Result<Vec<Permission>, AppError> {
    Ok(get(conn, role_id)?.permissions)
}

/// The `RoleInput` field (IPC name) of each name, in `[ar, fr, en]` order.
const NAME_FIELDS: [&str; 3] = ["nameAr", "nameFr", "nameEn"];

/// Trims the names and checks each one; sorts and deduplicates the permissions.
fn clean(mut input: RoleInput) -> Result<RoleInput, AppError> {
    input.name_ar = input.name_ar.trim().to_owned();
    input.name_fr = input.name_fr.trim().to_owned();
    input.name_en = input.name_en.trim().to_owned();
    let names = [&input.name_ar, &input.name_fr, &input.name_en];
    if names.iter().all(|n| n.is_empty()) {
        // One name in any language is enough; the default language's field reports it.
        return Err(AppError::validation("nameFr", "required"));
    }
    for (field, name) in NAME_FIELDS.into_iter().zip(names) {
        if name.chars().any(char::is_control) {
            return Err(AppError::validation(field, "value_invalid"));
        }
        if name.chars().count() > NAME_MAX {
            return Err(AppError::validation(field, "too_long"));
        }
    }
    normalize_permissions(&mut input.permissions);
    Ok(input)
}

/// Refuses a name that another active role (built-in or custom, `except` aside) already uses in
/// the same language, ignoring case. `to_lowercase` also folds accented letters (É/é), which
/// SQLite's ASCII-only NOCASE would not.
fn ensure_names_free(
    conn: &Connection,
    input: &RoleInput,
    except: Option<&str>,
) -> Result<(), AppError> {
    let mut stmt = conn.prepare(
        "SELECT name_ar, name_fr, name_en FROM roles WHERE archived_at IS NULL AND id IS NOT ?1",
    )?;
    let taken = stmt
        .query_map([except], |r| {
            Ok([r.get::<_, String>(0)?, r.get(1)?, r.get(2)?])
        })?
        .collect::<Result<Vec<[String; 3]>, _>>()?;
    let wanted = [&input.name_ar, &input.name_fr, &input.name_en];
    for (lang, (field, name)) in NAME_FIELDS.into_iter().zip(wanted).enumerate() {
        if name.is_empty() {
            continue;
        }
        let name = name.to_lowercase();
        if taken.iter().any(|other| other[lang].to_lowercase() == name) {
            return Err(AppError::validation(field, "role_name_taken"));
        }
    }
    Ok(())
}

/// What the audit journal records of a role: its three names and its permissions.
fn audit_snapshot(
    name_ar: &str,
    name_fr: &str,
    name_en: &str,
    permissions: &[Permission],
) -> Value {
    json!({ "nameAr": name_ar, "nameFr": name_fr, "nameEn": name_en, "permissions": permissions })
}

pub fn create(tx: &Transaction<'_>, actor: &Actor, input: RoleInput) -> Result<Role, AppError> {
    insert(tx, actor, input, None)
}

/// USR-015: a custom role with the permissions of `role_id` (system or custom), under new
/// `(ar, fr, en)` names. Journaled once, as `role.duplicated` with its source.
pub fn duplicate(
    tx: &Transaction<'_>,
    actor: &Actor,
    role_id: &str,
    names: (&str, &str, &str),
) -> Result<Role, AppError> {
    let source = get(tx, role_id)?;
    let input = RoleInput {
        name_ar: names.0.into(),
        name_fr: names.1.into(),
        name_en: names.2.into(),
        permissions: source.permissions,
    };
    insert(tx, actor, input, Some(role_id))
}

/// A new custom role, journaled as `role.created`, or as `role.duplicated` when copied `from`
/// another role.
fn insert(
    tx: &Transaction<'_>,
    actor: &Actor,
    input: RoleInput,
    from: Option<&str>,
) -> Result<Role, AppError> {
    let input = clean(input)?;
    ensure_names_free(tx, &input, None)?;
    let id = new_id();
    let now = now_ms();
    tx.execute(
        "INSERT INTO roles (id, code, name_ar, name_fr, name_en, is_system, permissions, created_at, updated_at)
         VALUES (?1, NULL, ?2, ?3, ?4, 0, ?5, ?6, ?6)",
        params![id, input.name_ar, input.name_fr, input.name_en, serde_json::to_string(&input.permissions)?, now],
    )?;
    let mut payload = audit_snapshot(
        &input.name_ar,
        &input.name_fr,
        &input.name_en,
        &input.permissions,
    );
    let action = match from {
        Some(source) => {
            payload["from"] = json!(source);
            "role.duplicated"
        }
        None => "role.created",
    };
    audit::append(tx, &actor.event(action, "role", Some(&id), payload))?;
    get(tx, &id)
}

pub fn update(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    input: RoleInput,
) -> Result<Role, AppError> {
    if id == OWNER_ROLE_ID {
        return Err(AppError::conflict("owner_role_locked"));
    }
    let before = get(tx, id)?;
    let input = clean(input)?;
    ensure_names_free(tx, &input, Some(id))?;
    tx.execute(
        "UPDATE roles SET name_ar = ?2, name_fr = ?3, name_en = ?4, permissions = ?5, updated_at = ?6 WHERE id = ?1",
        params![id, input.name_ar, input.name_fr, input.name_en, serde_json::to_string(&input.permissions)?, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event(
            "role.updated",
            "role",
            Some(id),
            json!({
                "before": audit_snapshot(
                    &before.name_ar,
                    &before.name_fr,
                    &before.name_en,
                    &before.permissions,
                ),
                "after": audit_snapshot(
                    &input.name_ar,
                    &input.name_fr,
                    &input.name_en,
                    &input.permissions,
                ),
            }),
        ),
    )?;
    get(tx, id)
}

/// Archives a custom role nobody holds, not even a deactivated user (the list shows the same
/// `user_count`). Built-in roles stay forever.
pub fn archive(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    let role = get(tx, id)?;
    if role.is_system {
        return Err(AppError::conflict("system_role_locked"));
    }
    if role.user_count > 0 {
        return Err(AppError::conflict("role_in_use"));
    }
    tx.execute(
        "UPDATE roles SET archived_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event(
            "role.archived",
            "role",
            Some(id),
            json!({ "nameAr": role.name_ar, "nameFr": role.name_fr, "nameEn": role.name_en }),
        ),
    )?;
    Ok(())
}

/// Every permission grouped for the role editor, in spec order.
pub fn catalog() -> Vec<PermissionGroupInfo> {
    PermissionGroup::ALL
        .into_iter()
        .map(|group| PermissionGroupInfo {
            group,
            permissions: Permission::ALL
                .into_iter()
                .filter(|p| p.group() == group)
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn seeded() -> Db {
        let db = Db::open_memory().unwrap();
        db.write(|tx| seed_system_roles(tx)).unwrap();
        db
    }

    fn input(name: &str, permissions: Vec<Permission>) -> RoleInput {
        RoleInput {
            name_ar: String::new(),
            name_fr: name.into(),
            name_en: String::new(),
            permissions,
        }
    }

    fn named(ar: &str, fr: &str, en: &str) -> RoleInput {
        RoleInput {
            name_ar: ar.into(),
            name_fr: fr.into(),
            name_en: en.into(),
            permissions: vec![],
        }
    }

    /// `(code, field)` of the error `create` returns for `input`.
    fn create_err(db: &Db, input: RoleInput) -> (String, Option<String>) {
        let payload = db
            .write(move |tx| create(tx, &Actor::default(), input))
            .unwrap_err()
            .payload();
        (payload.code, payload.field)
    }

    fn rejected(code: &str, field: &str) -> (String, Option<String>) {
        (code.to_owned(), Some(field.to_owned()))
    }

    /// Payload of the only audit row with `action`.
    fn audit_payload(db: &Db, action: &'static str) -> serde_json::Value {
        let text: String = db
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT payload FROM audit_log WHERE action = ?1",
                    [action],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        serde_json::from_str(&text).unwrap()
    }

    fn stored_as(db: &Db, role_id: &str, raw: &'static str) -> Vec<Permission> {
        let id = role_id.to_owned();
        db.write(move |tx| {
            tx.execute(
                "UPDATE roles SET permissions = ?2 WHERE id = ?1",
                (&id, raw),
            )?;
            Ok(())
        })
        .unwrap();
        let id = role_id.to_owned();
        db.read(move |c| permissions_of(c, &id)).unwrap()
    }

    #[test]
    fn seeding_is_idempotent_and_ordered() {
        let db = seeded();
        db.write(|tx| seed_system_roles(tx)).unwrap();
        let roles = db.read(list).unwrap();
        let ids: Vec<&str> = roles.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                OWNER_ROLE_ID,
                MANAGER_ROLE_ID,
                CASHIER_ROLE_ID,
                STOREKEEPER_ROLE_ID,
                WAITER_ROLE_ID,
                TECHNICIAN_ROLE_ID,
                ACCOUNTANT_ROLE_ID,
                EMPLOYEE_ROLE_ID
            ]
        );
        assert!(roles.iter().all(|r| r.is_system));
        assert_eq!(roles[2].name_fr, "Caissier");
    }

    #[test]
    fn trade_roles_hold_only_what_their_job_needs() {
        let db = seeded();
        let of = |id: &'static str| db.read(move |c| permissions_of(c, id)).unwrap();
        let waiter = of(WAITER_ROLE_ID);
        assert!(
            waiter.contains(&Permission::SalesCreate)
                && !waiter.contains(&Permission::SalesDiscount)
        );
        let accountant = of(ACCOUNTANT_ROLE_ID);
        assert!(
            accountant.contains(&Permission::ReportsTax)
                && !accountant.contains(&Permission::SalesCreate)
        );
        assert!(!of(TECHNICIAN_ROLE_ID).contains(&Permission::CashClose));
        assert!(!of(EMPLOYEE_ROLE_ID).contains(&Permission::SalesReturn));
    }

    #[test]
    fn owner_always_has_every_permission() {
        let db = seeded();
        db.write(|tx| {
            tx.execute(
                "UPDATE roles SET permissions = '[]' WHERE id = ?1",
                [OWNER_ROLE_ID],
            )?;
            Ok(())
        })
        .unwrap();
        let perms = db.read(|c| permissions_of(c, OWNER_ROLE_ID)).unwrap();
        assert_eq!(perms.len(), 46);
    }

    #[test]
    fn built_in_role_subsets() {
        let db = seeded();
        let cashier = db.read(|c| permissions_of(c, CASHIER_ROLE_ID)).unwrap();
        assert!(cashier.contains(&Permission::SalesCreate));
        assert!(!cashier.contains(&Permission::SalesDiscount));
        assert!(!cashier.contains(&Permission::CashViewExpected));
        let manager = db.read(|c| permissions_of(c, MANAGER_ROLE_ID)).unwrap();
        assert_eq!(manager.len(), 42);
        assert!(!manager.contains(&Permission::SettingsUsers));
        let storekeeper = db.read(|c| permissions_of(c, STOREKEEPER_ROLE_ID)).unwrap();
        assert_eq!(storekeeper.len(), 12);
    }

    #[test]
    fn custom_role_lifecycle_is_audited() {
        let db = seeded();
        let role = db
            .write(|tx| {
                create(
                    tx,
                    &Actor::default(),
                    input(
                        "Vendeur",
                        vec![
                            Permission::SalesHold,
                            Permission::SalesCreate,
                            Permission::SalesCreate,
                        ],
                    ),
                )
            })
            .unwrap();
        assert_eq!(
            role.permissions,
            [Permission::SalesCreate, Permission::SalesHold],
            "deduplicated, spec order"
        );
        let id = role.id.clone();
        let updated = db
            .write(move |tx| {
                update(
                    tx,
                    &Actor::default(),
                    &id,
                    RoleInput {
                        permissions: vec![Permission::SalesDiscount],
                        ..named("بائع", "Vendeur senior", "Senior seller")
                    },
                )
            })
            .unwrap();
        assert_eq!(updated.name_fr, "Vendeur senior");
        assert_eq!(updated.permissions, [Permission::SalesDiscount]);
        let id = role.id.clone();
        assert_eq!(
            db.read(move |c| permissions_of(c, &id)).unwrap(),
            [Permission::SalesDiscount],
            "the stored grant replaced the old one"
        );
        let id = role.id.clone();
        db.write(move |tx| archive(tx, &Actor::default(), &id))
            .unwrap();
        assert!(db.read(list).unwrap().iter().all(|r| r.id != role.id));
        let actions: Vec<String> = db
            .read(|c| {
                let mut s = c.prepare("SELECT action FROM audit_log ORDER BY seq")?;
                let v = s
                    .query_map([], |r| r.get(0))?
                    .collect::<Result<Vec<String>, _>>()?;
                Ok(v)
            })
            .unwrap();
        assert_eq!(actions, ["role.created", "role.updated", "role.archived"]);
        assert_eq!(
            audit_payload(&db, "role.created"),
            json!({
                "nameAr": "", "nameFr": "Vendeur", "nameEn": "",
                "permissions": ["sales.create", "sales.hold"],
            })
        );
        assert_eq!(
            audit_payload(&db, "role.updated"),
            json!({
                "before": {
                    "nameAr": "", "nameFr": "Vendeur", "nameEn": "",
                    "permissions": ["sales.create", "sales.hold"],
                },
                "after": {
                    "nameAr": "بائع", "nameFr": "Vendeur senior", "nameEn": "Senior seller",
                    "permissions": ["sales.discount"],
                },
            })
        );
        assert_eq!(
            audit_payload(&db, "role.archived"),
            json!({ "nameAr": "بائع", "nameFr": "Vendeur senior", "nameEn": "Senior seller" })
        );
    }

    #[test]
    fn unreadable_stored_permissions_fail_closed() {
        let db = seeded();
        let role = db
            .write(|tx| {
                create(
                    tx,
                    &Actor::default(),
                    input("Livreur", vec![Permission::SalesCreate]),
                )
            })
            .unwrap();
        assert_eq!(
            stored_as(&db, &role.id, r#"["sales.create","sales.fly"]"#),
            [Permission::SalesCreate],
            "an unknown name is dropped, the rest kept"
        );
        assert_eq!(
            stored_as(&db, &role.id, r#"["sales.create",5]"#),
            Vec::<Permission>::new(),
            "an unreadable list grants nothing"
        );
    }

    #[test]
    fn reseeding_keeps_an_edited_built_in_role() {
        let db = seeded();
        db.write(|tx| {
            update(
                tx,
                &Actor::default(),
                MANAGER_ROLE_ID,
                RoleInput {
                    permissions: vec![Permission::SalesCreate],
                    ..named("المسيّر", "Gérant", "Shift manager")
                },
            )
        })
        .unwrap();
        db.write(|tx| seed_system_roles(tx)).unwrap();
        let manager = db.read(|c| get(c, MANAGER_ROLE_ID)).unwrap();
        assert_eq!(manager.permissions, [Permission::SalesCreate]);
        assert_eq!(manager.name_en, "Shift manager");
    }

    #[test]
    fn name_length_and_control_characters_are_checked_per_language() {
        let db = seeded();
        // The limit counts characters, not bytes: 40 Arabic letters are 80 bytes.
        let at_limit = db
            .write(|tx| create(tx, &Actor::default(), named(&"ب".repeat(40), "", "")))
            .unwrap();
        assert_eq!(at_limit.name_ar.chars().count(), 40);
        assert_eq!(
            create_err(&db, named(&"ت".repeat(41), "", "")),
            rejected("too_long", "nameAr")
        );
        assert_eq!(
            create_err(&db, named("", &"x".repeat(41), "")),
            rejected("too_long", "nameFr")
        );
        assert_eq!(
            create_err(&db, named("", "", &"x".repeat(41))),
            rejected("too_long", "nameEn")
        );
        // Surrounding blanks are trimmed before counting.
        let padded = db
            .write(|tx| {
                create(
                    tx,
                    &Actor::default(),
                    named("", &format!("  {}  ", "y".repeat(40)), ""),
                )
            })
            .unwrap();
        assert_eq!(padded.name_fr, "y".repeat(40));
        assert_eq!(
            create_err(&db, named("", "Caisse\u{7}", "")),
            rejected("value_invalid", "nameFr")
        );
        assert_eq!(
            create_err(&db, named("", "", "Two\nlines")),
            rejected("value_invalid", "nameEn")
        );
        assert_eq!(
            create_err(&db, named("أمين\u{0}", "", "")),
            rejected("value_invalid", "nameAr")
        );
    }

    #[test]
    fn names_are_unique_per_language_among_active_roles() {
        let db = seeded();
        // Built-in names count, case-insensitively and beyond ASCII.
        assert_eq!(
            create_err(&db, named("", "caissier", "")),
            rejected("role_name_taken", "nameFr")
        );
        assert_eq!(
            create_err(&db, named("", "", "CASHIER")),
            rejected("role_name_taken", "nameEn")
        );
        assert_eq!(
            create_err(&db, named("أمين الصندوق", "", "")),
            rejected("role_name_taken", "nameAr")
        );
        let team = db
            .write(|tx| create(tx, &Actor::default(), named("", "Équipe", "")))
            .unwrap();
        assert_eq!(
            create_err(&db, named("", "équipe", "")),
            rejected("role_name_taken", "nameFr")
        );
        // Only the same language clashes: "Manager" is the English name of another role.
        db.write(|tx| create(tx, &Actor::default(), named("", "Manager", "")))
            .unwrap();
        // A role keeps its own name under a new case, but cannot take another role's.
        let id = team.id.clone();
        let recased = db
            .write(move |tx| update(tx, &Actor::default(), &id, named("", "ÉQUIPE", "")))
            .unwrap();
        assert_eq!(recased.name_fr, "ÉQUIPE");
        let id = team.id.clone();
        let taken = db
            .write(move |tx| update(tx, &Actor::default(), &id, named("", "Gérant", "")))
            .unwrap_err();
        assert_eq!(
            (taken.code(), taken.payload().field),
            rejected("role_name_taken", "nameFr")
        );
        // An archived role frees its names.
        let id = team.id.clone();
        db.write(move |tx| archive(tx, &Actor::default(), &id))
            .unwrap();
        db.write(|tx| create(tx, &Actor::default(), named("", "Équipe", "")))
            .unwrap();
    }

    #[test]
    fn locked_roles_and_validation() {
        let db = seeded();
        let owner_edit = db
            .write(|tx| {
                update(
                    tx,
                    &Actor::default(),
                    OWNER_ROLE_ID,
                    input("Patron", vec![]),
                )
            })
            .unwrap_err();
        assert_eq!(owner_edit.code(), "owner_role_locked");
        let system_archive = db
            .write(|tx| archive(tx, &Actor::default(), CASHIER_ROLE_ID))
            .unwrap_err();
        assert_eq!(system_archive.code(), "system_role_locked");
        let unnamed = db
            .write(|tx| create(tx, &Actor::default(), input("  ", vec![])))
            .unwrap_err();
        assert_eq!(
            (unnamed.code(), unnamed.payload().field),
            ("required".into(), Some("nameFr".into()))
        );
        let long = db
            .write(|tx| create(tx, &Actor::default(), input(&"x".repeat(41), vec![])))
            .unwrap_err();
        assert_eq!(long.code(), "too_long");
        assert_eq!(
            db.read(|c| get(c, "00000000-0000-7000-8000-00000000ffff"))
                .unwrap_err()
                .code(),
            "role_not_found"
        );
    }

    #[test]
    fn role_held_only_by_a_deactivated_user_is_still_in_use() {
        let db = seeded();
        let role = db
            .write(|tx| create(tx, &Actor::default(), input("Livreur", vec![])))
            .unwrap();
        let role_id = role.id.clone();
        db.write(move |tx| {
            tx.execute(
                "INSERT INTO users (id, username, display_name, pin_hash, role_id, active, created_at, updated_at)
                 VALUES (?1, 'livreur', 'Livreur', 'x', ?2, 0, 0, 0)",
                (crate::ids::new_id(), role_id),
            )?;
            Ok(())
        })
        .unwrap();
        let id = role.id.clone();
        let counted = db.read(move |c| get(c, &id)).unwrap();
        assert_eq!((counted.user_count, counted.active_user_count), (1, 0));
        let err = db
            .write(move |tx| archive(tx, &Actor::default(), &role.id))
            .unwrap_err();
        assert_eq!(err.code(), "role_in_use");
    }

    #[test]
    fn catalog_lists_every_permission_once_by_group() {
        let catalog = catalog();
        assert_eq!(catalog.len(), 10);
        assert_eq!(
            catalog.iter().map(|g| g.permissions.len()).sum::<usize>(),
            46
        );
    }

    #[test]
    fn catalog_is_exactly_all_permissions_in_spec_order() {
        assert_eq!(Permission::ALL.len(), 46);
        for p in Permission::ALL {
            assert_eq!(Permission::parse(p.as_str()), Some(p));
            let wire = format!("\"{}\"", p.as_str());
            assert_eq!(serde_json::from_str::<Permission>(&wire).unwrap(), p);
        }
        let listed: Vec<Permission> = catalog().into_iter().flat_map(|g| g.permissions).collect();
        assert_eq!(
            listed,
            Permission::ALL,
            "each permission once, in spec order"
        );
    }

    #[test]
    fn cashier_permissions_are_the_seeded_cashier_role_in_spec_order() {
        let db = seeded();
        let stored = db.read(|c| permissions_of(c, CASHIER_ROLE_ID)).unwrap();
        assert_eq!(stored, cashier_permissions());
        assert_eq!(cashier_permissions().len(), 11);
    }
}
