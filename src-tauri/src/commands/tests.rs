use super::*;
use crate::state::{AppPaths, AppState};
use pos_core::auth::{CASHIER_ROLE_ID, NewUser, OWNER_ROLE_ID, Permission, RoleInput, Session};
use pos_core::billing::archive::ArchiveQuery;
use pos_core::billing::convert::{ConvertInput, DeliveriesInput, TicketInvoiceInput};
use pos_core::billing::credit_notes::{CreditNoteInput, CreditReason};
use pos_core::billing::drafts::DraftInput;
use pos_core::billing::guard::GuardInput;
use pos_core::billing::print::PrintOptions;
use pos_core::billing::validate::ValidateInput;
use pos_core::cash::{CloseInput, ExpenseCategory, ManualKind};
use pos_core::catalog::bulk::{BulkEdit, BulkPriceInput, PriceChange};
use pos_core::catalog::categories::CategoryInput;
use pos_core::catalog::extras::ProductExtrasInput;
use pos_core::catalog::price_levels::PriceLevelInput;
use pos_core::catalog::pricing::PriceHelpInput;
use pos_core::catalog::products::ProductInput;
use pos_core::catalog::search::{CatalogQuery, SearchQuery};
use pos_core::catalog::tax_rates::TaxRateUpdate;
use pos_core::catalog::units::UnitInput;
use pos_core::catalog::variants::VariantsInput;
use pos_core::company::{CompanyInput, PriceMode, ShopActivity, TaxRegime};
use pos_core::customers::credit::CollectInput;
use pos_core::customers::{CustomerInput, CustomerQuery};
use pos_core::documents::{CreditKind, DocType, DocumentQuery};
use pos_core::expenses::{ExpenseCategoryInput, ExpenseQuery, NewExpense};
use pos_core::imports::spec::ImportEntity;
use pos_core::lang::Lang;
use pos_core::ledger::EntryQuery;
use pos_core::money::{Money, Qty, Rate};
use pos_core::payments::PaymentMethod;
use pos_core::promotions::{Campaign, PromoKind, PromoScope, PromotionInput, TriggerMode};
use pos_core::purchases::PurchaseQuery;
use pos_core::purchases::invoices::InvoiceInput;
use pos_core::purchases::orders::OrderInput;
use pos_core::purchases::receipts::ReceiptInput;
use pos_core::purchases::returns::SupplierReturnInput;
use pos_core::returns::ReturnInput;
use pos_core::sales::{Cart, CartEventInput, CartLine, SaleInput};
use pos_core::setup::FirstRunInput;
use pos_core::stock::counts::{CountInput, CountMode, CountScope, Uncounted};
use pos_core::stock::serials::SerialQuery;
use pos_core::stock::transfers::TransferInput;
use pos_core::stock::views::{LevelQuery, MovementQuery};
use pos_core::stock::{AdjustBatchInput, AdjustInput, AdjustReason, LocationInput};
use pos_core::suppliers::{SupplierInput, SupplierPaymentInput, SupplierQuery};
use pos_hw::config::{HardwareConfig, PrinterConfig};
use pos_hw::driver::Transport;
use pos_hw::profile::{PaperWidth, Preset, Profile};
use serde_json::json;
use std::path::Path;

pub(crate) fn first_run(st: &AppState) -> Session {
    setup::complete_first_run(
        st,
        FirstRunInput {
            lang: Lang::Fr,
            shop_name: "Supérette El Amel".into(),
            activity: ShopActivity::General,
            tax_regime: TaxRegime::Ifu,
            price_mode: PriceMode::Ttc,
            wilaya: Some("16".into()),
            phone: None,
            owner_name: "Amine Benali".into(),
            owner_pin: "2580".into(),
        },
    )
    .unwrap()
}

#[test]
fn first_run_logs_the_owner_in() {
    let st = AppState::for_tests();
    let owner = first_run(&st);
    assert!(owner.is_owner);
    assert_eq!(st.current_session().unwrap(), Some(owner));
}

#[test]
fn logout_then_login_with_the_pin() {
    let st = AppState::for_tests();
    let owner = first_run(&st);
    auth::logout(&st).unwrap();
    assert_eq!(st.current_session().unwrap(), None);
    let wrong = auth::login(
        &st,
        &auth::LoginInput {
            user: owner.user_id.clone(),
            pin: "0000".into(),
        },
    );
    assert_eq!(wrong.unwrap_err().code(), "invalid_pin");
    let again = auth::login(
        &st,
        &auth::LoginInput {
            user: owner.user_id,
            pin: "2580".into(),
        },
    )
    .unwrap();
    assert!(again.is_owner);
    assert_eq!(st.current_session().unwrap(), Some(again));
}

#[test]
fn language_change_updates_the_session() {
    let st = AppState::for_tests();
    first_run(&st);
    let arabic = auth::set_language(&st, Lang::Ar).unwrap();
    assert_eq!(arabic.lang, Lang::Ar);
    assert_eq!(st.session().unwrap().lang, Lang::Ar);
}

#[test]
fn override_needs_a_logged_in_requester() {
    let st = AppState::for_tests();
    let owner = first_run(&st);
    auth::logout(&st).unwrap();
    let input = auth::OverrideInput {
        approver_id: owner.user_id,
        pin: "2580".into(),
        permission: Permission::SalesDiscount,
    };
    assert_eq!(
        auth::manager_override(&st, &input).unwrap_err().code(),
        "session_required"
    );
}

#[test]
fn refresh_follows_user_edits_and_signs_out_a_deactivated_user() {
    let st = AppState::for_tests();
    let owner = first_run(&st);
    let (actor, owner_id) = (owner.actor(), owner.user_id.clone());
    st.db
        .write(move |tx| {
            let rename = pos_core::auth::UserUpdate {
                display_name: "Amine B.".into(),
                role_id: pos_core::auth::OWNER_ROLE_ID.into(),
                lang: Lang::En,
                discount_limit_bp: 10_000,
                active: true,
            };
            pos_core::auth::users::update(tx, &actor, &owner_id, &rename)
        })
        .unwrap();
    let renamed = st.refresh_session().unwrap().unwrap();
    assert_eq!(
        (renamed.display_name.as_str(), renamed.lang),
        ("Amine B.", Lang::En)
    );
    assert_eq!(renamed.started_at, owner.started_at, "same sign-in");
    assert_eq!(st.current_session().unwrap(), Some(renamed));

    let (actor, hash) = (owner.actor(), pos_core::auth::hash_pin("4826").unwrap());
    let cashier = st
        .db
        .write(move |tx| {
            let input = pos_core::auth::NewUser {
                username: None,
                display_name: "Yacine".into(),
                pin: String::new(),
                role_id: pos_core::auth::CASHIER_ROLE_ID.into(),
                lang: Lang::Fr,
                discount_limit_bp: 0,
            };
            pos_core::auth::users::create(tx, &actor, &input, &hash)
        })
        .unwrap();
    auth::logout(&st).unwrap();
    let login = auth::LoginInput {
        user: cashier.id.clone(),
        pin: "4826".into(),
    };
    auth::login(&st, &login).unwrap();
    let actor = owner.actor();
    st.db
        .write(move |tx| pos_core::auth::users::deactivate(tx, &actor, &cashier.id))
        .unwrap();
    assert_eq!(st.refresh_session().unwrap_err().code(), "session_invalid");
    assert_eq!(st.current_session().unwrap(), None, "signed out");
    assert_eq!(st.refresh_session().unwrap(), None, "nobody to refresh");
}

#[test]
fn login_input_debug_hides_the_pin() {
    let input = auth::LoginInput {
        user: "Yacine".into(),
        pin: "739184".into(),
    };
    let printed = format!("{input:?}");
    assert!(
        !printed.contains("739184") && printed.contains("<redacted>"),
        "{printed}"
    );
    assert!(printed.contains("Yacine"), "{printed}");
}

#[test]
fn override_input_debug_hides_the_pin() {
    let input = auth::OverrideInput {
        approver_id: "manager-7".into(),
        pin: "951357".into(),
        permission: Permission::SalesDiscount,
    };
    let printed = format!("{input:?}");
    assert!(
        !printed.contains("951357") && printed.contains("<redacted>"),
        "{printed}"
    );
    assert!(
        printed.contains("manager-7") && printed.contains("SalesDiscount"),
        "{printed}"
    );
}

fn add_user(st: &AppState, name: &str, role_id: &str, pin: &str) -> pos_core::auth::User {
    users::create_user(
        st,
        NewUser {
            username: None,
            display_name: name.into(),
            pin: pin.into(),
            role_id: role_id.into(),
            lang: Lang::Fr,
            discount_limit_bp: 0,
        },
    )
    .unwrap()
}

fn switch_to(st: &AppState, user_id: &str, pin: &str) {
    auth::logout(st).unwrap();
    auth::login(
        st,
        &auth::LoginInput {
            user: user_id.into(),
            pin: pin.into(),
        },
    )
    .unwrap();
}

#[test]
fn cashier_cannot_manage_users_or_read_the_audit() {
    let st = AppState::for_tests();
    first_run(&st);
    let cashier = add_user(&st, "Yacine", CASHIER_ROLE_ID, "4826");
    switch_to(&st, &cashier.id, "4826");
    assert_eq!(
        users::list_users(&st).unwrap_err().code(),
        "insufficient_permission"
    );
    let denied = users::create_user(
        &st,
        NewUser {
            username: None,
            display_name: "X".into(),
            pin: "7391".into(),
            role_id: CASHIER_ROLE_ID.into(),
            lang: Lang::Fr,
            discount_limit_bp: 0,
        },
    );
    assert_eq!(denied.unwrap_err().code(), "insufficient_permission");
    assert_eq!(
        audit::verify(&st).unwrap_err().code(),
        "insufficient_permission"
    );
    assert_eq!(
        company::update(&st, CompanyInput::named(Lang::Fr, "Autre"))
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
}

#[test]
fn public_settings_work_without_a_session_but_protected_ones_do_not() {
    let st = AppState::for_tests();
    assert_eq!(
        settings::get(&st, "ui.theme".into()).unwrap(),
        json!("system")
    );
    assert_eq!(
        settings::set(&st, "ui.login_lang".into(), json!("ar")).unwrap(),
        json!("ar")
    );
    assert_eq!(
        settings::get(&st, "security.auto_lock_minutes".into())
            .unwrap_err()
            .code(),
        "session_required"
    );
    assert_eq!(
        settings::set(&st, "security.auto_lock_minutes".into(), json!(5))
            .unwrap_err()
            .code(),
        "session_required"
    );
    first_run(&st);
    assert_eq!(
        settings::set(&st, "security.auto_lock_minutes".into(), json!(5)).unwrap(),
        json!(5)
    );
}

#[test]
fn owner_manages_roles_and_users_and_the_audit_verifies() {
    let st = AppState::for_tests();
    let owner = first_run(&st);
    let role = users::create_role(
        &st,
        RoleInput {
            name_ar: String::new(),
            name_fr: "Vendeur".into(),
            name_en: String::new(),
            permissions: vec![Permission::SalesCreate],
        },
    )
    .unwrap();
    let seller = add_user(&st, "Samir", &role.id, "7391");
    assert_eq!(users::list_users(&st).unwrap().len(), 2);
    users::set_pin(&st, seller.id.clone(), "4826".into()).unwrap();
    users::deactivate(&st, seller.id).unwrap();
    assert_eq!(
        users::delete_role(&st, role.id).unwrap_err().code(),
        "role_in_use"
    );
    let mut mine = pos_core::auth::UserUpdate {
        display_name: "Amine B.".into(),
        role_id: owner.role_id.clone(),
        lang: Lang::En,
        discount_limit_bp: 10_000,
        active: true,
    };
    let me = users::update_user(&st, owner.user_id.clone(), mine.clone()).unwrap();
    assert_eq!(me.display_name, "Amine B.");
    assert_eq!(
        st.session().unwrap().display_name,
        "Amine B.",
        "own session refreshed"
    );
    mine.active = false;
    assert_eq!(
        users::update_user(&st, owner.user_id, mine)
            .unwrap_err()
            .code(),
        "cannot_deactivate_self"
    );
    let report = audit::verify(&st).unwrap();
    assert!(report.rows > 5);
    assert_eq!(report.first_broken_seq, None);
    let page = audit::list(
        &st,
        pos_core::audit::AuditQuery {
            before_seq: None,
            limit: Some(3),
        },
    )
    .unwrap();
    assert_eq!(page.rows.len(), 3);
}

#[test]
fn owner_accounts_are_owner_only() {
    let st = AppState::for_tests();
    let owner = first_run(&st);
    let lead_role = users::create_role(
        &st,
        RoleInput {
            name_ar: String::new(),
            name_fr: "Chef d'équipe".into(),
            name_en: String::new(),
            permissions: vec![Permission::SettingsUsers],
        },
    )
    .unwrap();
    let lead = add_user(&st, "Karima", &lead_role.id, "7391");
    switch_to(&st, &lead.id, "7391");
    assert_eq!(
        users::set_pin(&st, owner.user_id.clone(), "4826".into())
            .unwrap_err()
            .code(),
        "owner_only"
    );
    let intruder = NewUser {
        username: None,
        display_name: "Intrus".into(),
        pin: "4826".into(),
        role_id: OWNER_ROLE_ID.into(),
        lang: Lang::Fr,
        discount_limit_bp: 0,
    };
    assert_eq!(
        users::create_user(&st, intruder).unwrap_err().code(),
        "owner_only"
    );
    let promote = pos_core::auth::UserUpdate {
        display_name: "Karima".into(),
        role_id: OWNER_ROLE_ID.into(),
        lang: Lang::Fr,
        discount_limit_bp: 0,
        active: true,
    };
    assert_eq!(
        users::update_user(&st, lead.id, promote)
            .unwrap_err()
            .code(),
        "owner_only"
    );
    assert_eq!(
        users::deactivate(&st, owner.user_id.clone())
            .unwrap_err()
            .code(),
        "owner_only"
    );
    switch_to(&st, &owner.user_id, "2580");
    users::set_pin(&st, owner.user_id, "4826".into()).unwrap();
}

#[test]
fn restore_is_owner_only_and_checks_the_pin() {
    let st = AppState::for_tests();
    first_run(&st);
    let keeper = users::create_role(
        &st,
        RoleInput {
            name_ar: String::new(),
            name_fr: "Sauvegardes".into(),
            name_en: String::new(),
            permissions: vec![Permission::SettingsData],
        },
    )
    .unwrap();
    let saver = add_user(&st, "Nadia", &keeper.id, "7391");
    let owner_id = st.session().unwrap().user_id;
    assert_eq!(
        backup::restore(&st, "pos-x.db", "0000").unwrap_err().code(),
        "invalid_pin"
    );
    switch_to(&st, &saver.id, "7391");
    assert_eq!(
        backup::restore(&st, "pos-x.db", "7391").unwrap_err().code(),
        "owner_only"
    );
    switch_to(&st, &owner_id, "2580");
    assert_eq!(
        backup::restore(&st, "pos-x.db", "2580").unwrap_err().code(),
        "backup_not_found"
    );
}

fn role_input(name_fr: &str, permissions: Vec<Permission>) -> RoleInput {
    RoleInput {
        name_ar: String::new(),
        name_fr: name_fr.into(),
        name_en: String::new(),
        permissions,
    }
}

/// Spec §4.5: the owner creates custom roles; a `settings.users` holder
/// manages users and assigns the existing roles, nothing more.
#[test]
fn roles_are_created_changed_and_removed_by_owners_only() {
    let st = AppState::for_tests();
    let owner = first_run(&st);
    let lead_role = users::create_role(
        &st,
        role_input("Chef d'équipe", vec![Permission::SettingsUsers]),
    )
    .unwrap();
    let spare =
        users::create_role(&st, role_input("Vendeur", vec![Permission::SalesCreate])).unwrap();
    let lead = add_user(&st, "Karima", &lead_role.id, "7391");
    let cashier = add_user(&st, "Yacine", CASHIER_ROLE_ID, "4826");
    switch_to(&st, &lead.id, "7391");
    assert_eq!(
        users::list_roles(&st).unwrap().len(),
        10,
        "the eight built-in roles and two custom ones"
    );
    assert_eq!(
        users::role_catalog(&st).unwrap().len(),
        10,
        "one entry per permission group"
    );
    let grab = role_input("Chef d'équipe", Permission::ALL.to_vec());
    assert_eq!(
        users::create_role(&st, role_input("Gérant bis", Permission::ALL.to_vec()))
            .unwrap_err()
            .code(),
        "owner_only"
    );
    assert_eq!(
        users::update_role(&st, lead_role.id.clone(), grab)
            .unwrap_err()
            .code(),
        "owner_only"
    );
    assert_eq!(
        users::delete_role(&st, spare.id.clone())
            .unwrap_err()
            .code(),
        "owner_only"
    );
    assert!(
        !st.session().unwrap().has(Permission::SettingsData),
        "the lead's own role is unchanged"
    );
    switch_to(&st, &cashier.id, "4826");
    assert_eq!(
        users::list_roles(&st).unwrap_err().code(),
        "insufficient_permission"
    );
    assert_eq!(
        users::role_catalog(&st).unwrap_err().code(),
        "insufficient_permission"
    );
    switch_to(&st, &owner.user_id, "2580");
    users::delete_role(&st, spare.id).unwrap();
    assert_eq!(
        users::list_roles(&st).unwrap().len(),
        9,
        "archived, no longer listed"
    );
}

#[test]
fn a_users_edit_applies_to_the_callers_own_session_at_once() {
    let st = AppState::for_tests();
    first_run(&st);
    let lead_role = users::create_role(
        &st,
        role_input("Chef d'équipe", vec![Permission::SettingsUsers]),
    )
    .unwrap();
    let lead = add_user(&st, "Karima", &lead_role.id, "7391");
    switch_to(&st, &lead.id, "7391");
    let demote = pos_core::auth::UserUpdate {
        display_name: "Karima".into(),
        role_id: CASHIER_ROLE_ID.into(),
        lang: Lang::Ar,
        discount_limit_bp: 0,
        active: true,
    };
    users::update_user(&st, lead.id, demote).unwrap();
    let session = st.session().unwrap();
    assert_eq!(
        (session.role_id.as_str(), session.lang),
        (CASHIER_ROLE_ID, Lang::Ar)
    );
    assert!(!session.has(Permission::SettingsUsers));
    assert_eq!(
        users::list_users(&st).unwrap_err().code(),
        "insufficient_permission"
    );
}

#[test]
fn the_logo_needs_settings_company_and_company_get_reports_it() {
    let st = AppState::for_tests();
    first_run(&st);
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    assert!(company::set_logo(&st, Some(png)).unwrap().has_logo);
    assert!(
        st.db
            .read(pos_core::company::get)
            .unwrap()
            .unwrap()
            .has_logo
    );
    assert_eq!(
        company::set_logo(&st, Some(b"GIF89a".to_vec()))
            .unwrap_err()
            .code(),
        "logo_invalid"
    );
    let cashier = add_user(&st, "Yacine", CASHIER_ROLE_ID, "4826");
    switch_to(&st, &cashier.id, "4826");
    assert_eq!(
        company::set_logo(&st, None).unwrap_err().code(),
        "insufficient_permission"
    );
    assert!(
        st.db
            .read(pos_core::company::get)
            .unwrap()
            .unwrap()
            .has_logo
    );
}

/// Once the restore is scheduled, this database is closed to writes at
/// once, so nothing written before the restart can be lost behind the swap.
#[test]
fn a_scheduled_restore_closes_the_database_to_writes() {
    let dir = tempfile::tempdir().unwrap();
    let st = AppState::open(&AppPaths::new(dir.path()), crate::printing::silent()).unwrap();
    first_run(&st);
    let made = backup::backup_now_impl(&st).unwrap();
    assert_eq!(backup::list(&st).unwrap()[0].file_name, made.file_name);
    let info = backup::restore(&st, &made.file_name, "2580").unwrap();
    assert_eq!(info.file_name, made.file_name);
    assert!(dir.path().join(pos_core::db::RESTORE_MARKER).exists());
    let late = settings::set(&st, "ui.theme".into(), json!("dark"));
    assert_eq!(late.unwrap_err().code(), "internal_error");
    let actions: Vec<String> = st
        .db
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT action FROM audit_log WHERE action LIKE 'backup.%' ORDER BY seq",
            )?;
            let rows = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap();
    assert_eq!(actions, ["backup.created", "backup.restore_scheduled"]);
}

#[test]
fn setting_values_cross_the_ipc_as_plain_json() {
    let value: settings::SettingValue = serde_json::from_value(json!({ "minutes": 5 })).unwrap();
    assert_eq!(value, settings::SettingValue(json!({ "minutes": 5 })));
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        json!({ "minutes": 5 })
    );
}
/// `(user_id, payload)` of every audit row with `action`, oldest first.
fn audit_rows(st: &AppState, action: &'static str) -> Vec<(Option<String>, serde_json::Value)> {
    st.db
        .read(move |c| {
            let mut stmt =
                c.prepare("SELECT user_id, payload FROM audit_log WHERE action = ?1 ORDER BY seq")?;
            let rows = stmt
                .query_map([action], |r| {
                    Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap()
        .into_iter()
        .map(|(user, payload)| (user, serde_json::from_str(&payload).unwrap()))
        .collect()
}

/// `(action, user_id)` of the last `n` audit rows, oldest first.
fn last_actions(st: &AppState, n: usize) -> Vec<(String, Option<String>)> {
    let mut rows: Vec<(String, Option<String>)> = st
        .db
        .read(|c| {
            let mut stmt = c.prepare("SELECT action, user_id FROM audit_log ORDER BY seq DESC")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap();
    rows.truncate(n);
    rows.reverse();
    rows
}

/// Review finding: the sign-out fails closed. A journal that refuses the write (the database
/// closed for shutdown here; a full disk in a shop) must not leave the user signed in.
#[test]
fn logout_signs_out_even_when_the_journal_refuses_the_write() {
    let st = AppState::for_tests();
    first_run(&st);
    st.db.mark_clean_shutdown().unwrap();
    auth::logout(&st).unwrap();
    assert_eq!(st.current_session().unwrap(), None);
    auth::logout(&st).unwrap(); // nobody signed in: nothing to do
}

#[test]
fn a_sign_in_over_another_users_session_journals_that_users_sign_out() {
    let st = AppState::for_tests();
    let owner = first_run(&st);
    let cashier = add_user(&st, "Yacine", CASHIER_ROLE_ID, "4826");
    let login = auth::LoginInput {
        user: cashier.id.clone(),
        pin: "4826".into(),
    };
    auth::login(&st, &login).unwrap(); // the owner never signed out
    assert_eq!(
        last_actions(&st, 2),
        [
            ("auth.login".to_owned(), Some(cashier.id.clone())),
            ("auth.logout".to_owned(), Some(owner.user_id)),
        ]
    );
    auth::login(&st, &login).unwrap();
    assert_eq!(
        last_actions(&st, 1),
        [("auth.login".to_owned(), Some(cashier.id))],
        "signing in again as the same user signs nobody out"
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Guard {
    /// Works without a session (login screen, setup, startup).
    Public,
    /// Needs someone signed in, whatever the role.
    Session,
    /// Needs a permission the cashier role holds (catalogue and stock reads the till shares).
    Cashier,
    /// Needs a permission the cashier role lacks.
    Permission,
    /// Needs an owner, even with every permission.
    Owner,
}

/// Calls the command's body as the signed-in user (or nobody) with harmless arguments.
type Probe = fn(&AppState) -> Result<(), pos_core::error::AppError>;

const NO_ID: &str = "00000000-0000-7000-8000-00000000dead";

/// Every command and its guard, pinned (spec §4.5: permission checks happen in Rust). A new
/// command fails `every_command_pins_its_guard` until it is added here with a probe.
fn guards() -> Vec<(&'static str, Guard, Option<Probe>)> {
    use Guard::{Cashier, Owner, Public, Session};
    let permission = Guard::Permission;
    vec![
        ("app_ping", Public, None),
        ("app_info", Public, None),
        ("app_data_dir", Public, None),
        ("app_startup_error", Public, None),
        ("app_startup_notices", Public, None),
        (
            "app_open_folder",
            permission,
            Some(|st| app::may_open_folder(Some(st), false)),
        ),
        ("app_ready", Public, None),
        ("app_open_link", Public, None),
        ("app_restart", permission, Some(app::may_restart)),
        ("datapack_get", Public, None),
        ("setup_status", Public, None),
        ("setup_complete_first_run", Public, None),
        ("auth_list_users", Public, None),
        ("auth_login", Public, None),
        ("auth_logout", Public, None),
        ("auth_session", Public, None),
        (
            "auth_manager_override",
            Session,
            Some(|st| {
                let input = auth::OverrideInput {
                    approver_id: NO_ID.into(),
                    pin: "0000".into(),
                    permission: Permission::SalesDiscount,
                };
                auth::manager_override(st, &input).map(drop)
            }),
        ),
        (
            "auth_set_language",
            Session,
            Some(|st| auth::set_language(st, Lang::Fr).map(drop)),
        ),
        ("company_get", Public, None),
        (
            "company_update",
            permission,
            Some(|st| company::update(st, CompanyInput::named(Lang::Fr, "Boutique")).map(drop)),
        ),
        (
            "company_set_logo",
            permission,
            Some(|st| company::set_logo(st, None).map(drop)),
        ),
        // `ui.*` keys are public (login screen language and theme); the others need a session.
        (
            "settings_get",
            Session,
            Some(|st| settings::get(st, "security.auto_lock_minutes".into()).map(drop)),
        ),
        (
            "settings_set",
            permission,
            Some(|st| settings::set(st, "security.auto_lock_minutes".into(), json!(5)).map(drop)),
        ),
        (
            "users_list",
            permission,
            Some(|st| users::list_users(st).map(drop)),
        ),
        (
            "users_create",
            permission,
            Some(|st| {
                let input = NewUser {
                    username: None,
                    display_name: "Sonde".into(),
                    pin: "7391".into(),
                    role_id: CASHIER_ROLE_ID.into(),
                    lang: Lang::Fr,
                    discount_limit_bp: 0,
                };
                users::create_user(st, input).map(drop)
            }),
        ),
        (
            "users_update",
            permission,
            Some(|st| {
                let input = pos_core::auth::UserUpdate {
                    display_name: "Sonde".into(),
                    role_id: CASHIER_ROLE_ID.into(),
                    lang: Lang::Fr,
                    discount_limit_bp: 0,
                    active: true,
                };
                users::update_user(st, NO_ID.into(), input).map(drop)
            }),
        ),
        (
            "users_set_pin",
            permission,
            Some(|st| users::set_pin(st, NO_ID.into(), "7391".into())),
        ),
        (
            "users_deactivate",
            permission,
            Some(|st| users::deactivate(st, NO_ID.into())),
        ),
        (
            "roles_list",
            permission,
            Some(|st| users::list_roles(st).map(drop)),
        ),
        (
            "roles_catalog",
            permission,
            Some(|st| users::role_catalog(st).map(drop)),
        ),
        (
            "roles_create",
            Owner,
            Some(|st| users::create_role(st, role_input("Sonde", vec![])).map(drop)),
        ),
        (
            "roles_update",
            Owner,
            Some(|st| users::update_role(st, NO_ID.into(), role_input("Sonde", vec![])).map(drop)),
        ),
        (
            "roles_delete",
            Owner,
            Some(|st| users::delete_role(st, NO_ID.into())),
        ),
        (
            "audit_list",
            permission,
            Some(|st| audit::list(st, pos_core::audit::AuditQuery::default()).map(drop)),
        ),
        (
            "audit_verify_chain",
            permission,
            Some(|st| audit::verify(st).map(drop)),
        ),
        (
            "audit_history",
            permission,
            Some(|st| audit::history(st, pos_core::audit::AuditFilter::default()).map(drop)),
        ),
        (
            "backup_now",
            permission,
            Some(|st| backup::backup_now_impl(st).map(drop)),
        ),
        (
            "backup_list",
            permission,
            Some(|st| backup::list(st).map(drop)),
        ),
        (
            "backup_status",
            Session,
            Some(|st| backup::status(st).map(drop)),
        ),
        (
            "backup_restore",
            Owner,
            Some(|st| backup::restore(st, "pos-x.db", "0000").map(drop)),
        ),
        (
            "backup_mirror_status",
            permission,
            Some(|st| backup::mirror_status(st).map(drop)),
        ),
        (
            "backup_mirror_choose",
            permission,
            Some(|st| st.require(Permission::SettingsData).map(drop)),
        ),
        (
            "backup_mirror_clear",
            permission,
            Some(|st| backup::mirror_clear(st).map(drop)),
        ),
        (
            "backup_drives",
            permission,
            Some(|st| st.require(Permission::SettingsData).map(drop)),
        ),
        (
            "backup_to_drive",
            permission,
            Some(|st| {
                backup::to_drive(st, "Z:\\no-such-drive")
                    .map(drop)
                    .or_else(|e| {
                        if e.code() == "mirror_unavailable" {
                            Ok(())
                        } else {
                            Err(e)
                        }
                    })
            }),
        ),
        (
            "backup_inspect",
            permission,
            Some(|st| {
                backup::inspect(st, "none.db").map(drop).or_else(|e| {
                    if e.code() == "backup_not_found" {
                        Ok(())
                    } else {
                        Err(e)
                    }
                })
            }),
        ),
        (
            "backup_check_database",
            permission,
            Some(|st| backup::check_database(st).map(drop)),
        ),
        ("recovery_list", Public, None),
        ("recovery_inspect", Public, None),
        ("recovery_restore", Public, None),
        ("recovery_rejoin", Public, None),
        ("recovery_start_fresh", Public, None),
        (
            "catalog_units_list",
            Cashier,
            Some(|st| catalog::units_list(st).map(drop)),
        ),
        (
            "catalog_units_create",
            permission,
            Some(|st| catalog::units_create(st, UnitInput::default()).map(drop)),
        ),
        (
            "catalog_units_update",
            permission,
            Some(|st| catalog::units_update(st, NO_ID.into(), UnitInput::default()).map(drop)),
        ),
        (
            "catalog_categories_list",
            Cashier,
            Some(|st| catalog::categories_list(st, true).map(drop)),
        ),
        (
            "catalog_categories_create",
            permission,
            Some(|st| catalog::categories_create(st, CategoryInput::default()).map(drop)),
        ),
        (
            "catalog_categories_update",
            permission,
            Some(|st| {
                catalog::categories_update(st, NO_ID.into(), CategoryInput::default()).map(drop)
            }),
        ),
        (
            "catalog_categories_archive",
            permission,
            Some(|st| catalog::categories_archive(st, NO_ID.into())),
        ),
        (
            "catalog_tax_rates_list",
            Cashier,
            Some(|st| catalog::tax_rates_list(st).map(drop)),
        ),
        (
            "catalog_tax_rates_update",
            permission,
            Some(|st| {
                catalog::tax_rates_update(st, NO_ID.into(), TaxRateUpdate::default()).map(drop)
            }),
        ),
        (
            "catalog_price_levels_list",
            Cashier,
            Some(|st| catalog::price_levels_list(st).map(drop)),
        ),
        (
            "catalog_price_levels_create",
            permission,
            Some(|st| catalog::price_levels_create(st, PriceLevelInput::default()).map(drop)),
        ),
        (
            "catalog_price_levels_update",
            permission,
            Some(|st| {
                catalog::price_levels_update(st, NO_ID.into(), PriceLevelInput::default()).map(drop)
            }),
        ),
        (
            "catalog_price_levels_set_default",
            permission,
            Some(|st| catalog::price_levels_set_default(st, NO_ID.into()).map(drop)),
        ),
        (
            "catalog_price_levels_archive",
            permission,
            Some(|st| catalog::price_levels_archive(st, NO_ID.into())),
        ),
        (
            "products_get",
            Cashier,
            Some(|st| products::get(st, NO_ID.into()).map(drop)),
        ),
        (
            "products_create",
            permission,
            Some(|st| products::create(st, probe_product()).map(drop)),
        ),
        (
            "products_update",
            permission,
            Some(|st| products::update(st, NO_ID.into(), probe_product()).map(drop)),
        ),
        (
            "products_archive",
            permission,
            Some(|st| products::archive(st, NO_ID.into())),
        ),
        (
            "products_restore",
            permission,
            Some(|st| products::restore(st, NO_ID.into())),
        ),
        (
            "products_duplicate",
            permission,
            Some(|st| products::duplicate(st, NO_ID.into()).map(drop)),
        ),
        (
            "products_set_price",
            permission,
            Some(|st| products::set_price(st, NO_ID.into(), NO_ID.into(), Money::ZERO).map(drop)),
        ),
        (
            "products_search",
            Cashier,
            Some(|st| products::search(st, SearchQuery::default()).map(drop)),
        ),
        (
            "products_find_by_code",
            Cashier,
            Some(|st| products::find_by_code(st, "0000".into()).map(drop)),
        ),
        (
            "products_grid",
            Cashier,
            Some(|st| products::grid(st, None, None).map(drop)),
        ),
        (
            "products_catalog",
            Cashier,
            Some(|st| products::catalog(st, CatalogQuery::default()).map(drop)),
        ),
        (
            "products_variants",
            Cashier,
            Some(|st| products::variants(st, NO_ID.into()).map(drop)),
        ),
        (
            "stock_locations",
            Cashier,
            Some(|st| stock::locations(st).map(drop)),
        ),
        (
            "stock_create_location",
            permission,
            Some(|st| stock::create_location(st, LocationInput::default()).map(drop)),
        ),
        (
            "stock_levels",
            Cashier,
            Some(|st| stock::levels(st, NO_ID.into()).map(drop)),
        ),
        (
            "stock_movements",
            Cashier,
            Some(|st| stock::movements(st, NO_ID.into(), 10).map(drop)),
        ),
        (
            "stock_low",
            Cashier,
            Some(|st| stock::low(st, 10).map(drop)),
        ),
        (
            "stock_adjust",
            permission,
            Some(|st| {
                let input = AdjustInput {
                    product_id: NO_ID.into(),
                    location_id: None,
                    qty_delta: Qty(1_000),
                    reason: AdjustReason::Opening,
                    unit_cost: None,
                    note: None,
                };
                stock::adjust(st, input).map(drop)
            }),
        ),
        (
            "stock_rebuild_levels",
            permission,
            Some(|st| stock::rebuild_levels(st).map(drop)),
        ),
        (
            "hardware_get",
            Session,
            Some(|st| hardware::get(st).map(drop)),
        ),
        (
            "hardware_save",
            permission,
            Some(|st| hardware::save(st, probe_hardware()).map(drop)),
        ),
        (
            "hardware_discover",
            permission,
            Some(|st| hardware::discover_devices(st).map(drop)),
        ),
        (
            "hardware_test_print",
            permission,
            Some(|st| hardware::test_print(st, probe_printer(), Lang::Fr).map(drop)),
        ),
        (
            "hardware_open_drawer",
            Cashier,
            Some(|st| {
                let input = hardware::NoSaleInput {
                    reason: hardware::NoSaleReason::ChangeRequest,
                    note: None,
                };
                hardware::open_drawer(st, input)
            }),
        ),
        (
            "hardware_retry_print",
            Cashier,
            Some(|st| hardware::retry_print(st, NO_ID.into())),
        ),
        (
            "documents_list",
            Cashier,
            Some(|st| documents::list(st, DocumentQuery::default()).map(drop)),
        ),
        (
            "documents_get",
            Cashier,
            Some(|st| documents::get(st, NO_ID.into()).map(drop)),
        ),
        (
            "documents_find",
            Cashier,
            Some(|st| documents::find(st, "TK2026-C1-000000".into()).map(drop)),
        ),
        (
            "documents_reprint",
            Cashier,
            Some(|st| documents::reprint(st, NO_ID.into(), None).map(drop)),
        ),
        (
            "documents_verify_chain",
            permission,
            Some(|st| documents::verify_chain(st).map(drop)),
        ),
        (
            "documents_preview_draft",
            permission,
            Some(|st| billing::preview(st, probe_draft()).map(drop)),
        ),
        (
            "documents_save_draft",
            permission,
            Some(|st| billing::save_draft(st, None, probe_draft()).map(drop)),
        ),
        (
            "documents_draft_cart",
            permission,
            Some(|st| billing::draft_cart(st, NO_ID.into()).map(drop)),
        ),
        (
            "documents_cancel_draft",
            permission,
            Some(|st| billing::cancel_draft(st, NO_ID.into())),
        ),
        (
            "documents_validate",
            permission,
            Some(|st| {
                let input = ValidateInput {
                    id: NO_ID.into(),
                    payments: Vec::new(),
                };
                billing::validate(st, input).map(drop)
            }),
        ),
        (
            "documents_payment_preview",
            permission,
            Some(|st| billing::payment_preview(st, NO_ID.into(), Vec::new()).map(drop)),
        ),
        (
            "documents_convert",
            permission,
            Some(|st| {
                let input = ConvertInput {
                    source_id: NO_ID.into(),
                    target: DocType::Invoice,
                    reprice: false,
                };
                billing::convert(st, input).map(drop)
            }),
        ),
        (
            "documents_deliveries_preview",
            permission,
            Some(|st| billing::deliveries_preview(st, vec![NO_ID.into()], Vec::new()).map(drop)),
        ),
        (
            "documents_invoice_deliveries",
            permission,
            Some(|st| {
                let input = DeliveriesInput {
                    delivery_ids: vec![NO_ID.into()],
                    payments: Vec::new(),
                    due_date: None,
                    second_lang: None,
                    note: None,
                };
                billing::invoice_deliveries(st, input).map(drop)
            }),
        ),
        (
            "documents_invoice_ticket",
            permission,
            Some(|st| {
                let input = TicketInvoiceInput {
                    ticket_id: NO_ID.into(),
                    customer_id: NO_ID.into(),
                    second_lang: None,
                    note: None,
                };
                billing::invoice_ticket(st, input).map(drop)
            }),
        ),
        (
            "documents_link_sale",
            Cashier,
            Some(|st| billing::link_sale(st, NO_ID.into(), NO_ID.into())),
        ),
        (
            "documents_offer_cart",
            Cashier,
            Some(|st| billing::offer_cart(st, NO_ID.into()).map(drop)),
        ),
        (
            "documents_creditable",
            permission,
            Some(|st| billing::creditable(st, NO_ID.into()).map(drop)),
        ),
        (
            "documents_credit_quote",
            permission,
            Some(|st| billing::credit_quote(st, probe_credit()).map(drop)),
        ),
        (
            "documents_create_credit_note",
            permission,
            Some(|st| billing::create_credit_note(st, probe_credit()).map(drop)),
        ),
        (
            "documents_search",
            Cashier,
            Some(|st| billing::search(st, ArchiveQuery::default()).map(drop)),
        ),
        (
            "documents_state",
            Cashier,
            Some(|st| billing::state(st, NO_ID.into()).map(drop)),
        ),
        (
            "documents_links",
            Cashier,
            Some(|st| billing::links(st, NO_ID.into()).map(drop)),
        ),
        (
            "documents_mentions",
            permission,
            Some(|st| billing::mentions(st, None).map(drop)),
        ),
        (
            "documents_arabic_guard",
            Session,
            Some(|st| {
                let input = GuardInput {
                    lang: Lang::Fr,
                    second_lang: None,
                    product_ids: Vec::new(),
                    consumer: true,
                };
                billing::arabic_guard(st, input).map(drop)
            }),
        ),
        (
            "documents_warranty_lines",
            Cashier,
            Some(|st| billing::warranty_lines(st, NO_ID.into()).map(drop)),
        ),
        (
            "cash_current",
            Session,
            Some(|st| cash::current(st).map(drop)),
        ),
        (
            "cash_open_session",
            Cashier,
            Some(|st| cash::open_session(st, Money(-1)).map(drop)),
        ),
        (
            "cash_move",
            Cashier,
            Some(|st| {
                let input = cash::CashMoveInput {
                    kind: ManualKind::PaidIn,
                    amount: Money::ZERO,
                    note: None,
                };
                cash::move_cash(st, input).map(drop)
            }),
        ),
        (
            "cash_expense",
            Cashier,
            Some(|st| {
                let input = cash::ExpenseInput {
                    category: ExpenseCategory::Other,
                    amount: Money::ZERO,
                    note: None,
                };
                cash::expense(st, input).map(drop)
            }),
        ),
        (
            "cash_movements",
            permission,
            Some(|st| cash::movements(st, NO_ID.into()).map(drop)),
        ),
        (
            "cash_x_report",
            Cashier,
            Some(|st| cash::x_report(st).map(drop)),
        ),
        (
            "cash_close_session",
            Cashier,
            Some(|st| cash::close_session(st, CloseInput::default()).map(drop)),
        ),
        (
            "cash_close_offline",
            Cashier,
            Some(|st| cash::close_offline(st, CloseInput::default()).map(drop)),
        ),
        // M5: the role changes (they need the `SharedState` of the runtime) and promote/leave
        // (whose PIN check would count failures against the owner) are pinned by their first
        // line, `may_change_role`; joining by `may_join`, never by its network body; the
        // firewall by `may_see_firewall`, which runs no PowerShell.
        ("lan_status", Session, Some(|st| lan::status(st).map(drop))),
        ("lan_sync_now", Session, Some(lan::sync_now)),
        ("lan_become_host", Owner, Some(lan::may_change_role)),
        ("lan_become_standalone", Owner, Some(lan::may_change_role)),
        (
            "lan_pairing_code",
            permission,
            Some(|st| lan::pairing_code(st).map(drop)),
        ),
        ("lan_cancel_pairing", permission, Some(lan::cancel_pairing)),
        ("lan_tills", permission, Some(|st| lan::tills(st).map(drop))),
        (
            "lan_revoke_till",
            Owner,
            Some(|st| lan::revoke_till(st, NO_ID.into())),
        ),
        (
            "lan_till_closures",
            permission,
            Some(|st| lan::till_closures(st, NO_ID.into(), 10).map(drop)),
        ),
        (
            "lan_notices",
            permission,
            Some(|st| lan::notices(st, true).map(drop)),
        ),
        (
            "lan_acknowledge_notice",
            permission,
            Some(|st| lan::acknowledge_notice(st, NO_ID.into())),
        ),
        ("lan_firewall", permission, Some(lan::may_see_firewall)),
        ("lan_discover", Owner, Some(lan::may_join)),
        ("lan_probe", Owner, Some(lan::may_join)),
        ("lan_join", Owner, Some(lan::may_join)),
        ("lan_restart", Owner, Some(lan::may_join)),
        ("lan_retry_rejected", permission, Some(lan::retry_rejected)),
        (
            "lan_pending_entries",
            permission,
            Some(|st| crate::lan::registers::pending_entries(st).map(drop)),
        ),
        (
            "lan_pending_retry",
            permission,
            Some(|st| crate::lan::registers::retry_pending(st).map(drop)),
        ),
        ("lan_promote", Owner, Some(lan::may_change_role)),
        ("lan_leave", Owner, Some(lan::may_change_role)),
        (
            "lan_search_host",
            Cashier,
            Some(|st| {
                let query = SearchQuery {
                    text: "lait".into(),
                    limit: None,
                    category_id: None,
                    include_archived: false,
                };
                lan::search_host(st, query).map(drop)
            }),
        ),
        ("lan_confirm_close", Public, None),
        (
            "cash_z_report",
            Cashier,
            Some(|st| cash::z_report(st, NO_ID.into()).map(drop)),
        ),
        (
            "cash_sessions",
            permission,
            Some(|st| cash::sessions(st, 1).map(drop)),
        ),
        (
            "cash_verify_closures",
            permission,
            Some(|st| cash::verify_closures(st).map(drop)),
        ),
        (
            "lan_verify_journals",
            permission,
            Some(|st| lan::verify_journals(st).map(drop)),
        ),
        (
            "cash_print_z",
            Cashier,
            Some(|st| cash::print_z(st, NO_ID.into(), Lang::Fr).map(drop)),
        ),
        (
            "cash_print_x",
            Cashier,
            Some(|st| cash::print_x(st, Lang::Fr).map(drop)),
        ),
        (
            "sales_preview",
            Cashier,
            Some(|st| {
                let input = sales::PreviewInput {
                    cart: Cart::new(Lang::Fr, vec![]),
                    tenders: vec![],
                    store_credit: None,
                };
                sales::preview(st, input).map(drop)
            }),
        ),
        (
            "sales_points_preview",
            Cashier,
            Some(|st| sales::points_preview(st, NO_ID.into(), Money::ZERO, vec![]).map(drop)),
        ),
        (
            "sales_create",
            Cashier,
            Some(|st| {
                let request = sales::SaleRequest {
                    sale: probe_sale(),
                    print_receipt: false,
                    extras: Default::default(),
                };
                sales::create(st, request).map(drop)
            }),
        ),
        (
            "sales_hold",
            Cashier,
            Some(|st| sales::hold(st, "Sonde".into(), Cart::new(Lang::Fr, vec![])).map(drop)),
        ),
        ("sales_held", Cashier, Some(|st| sales::held(st).map(drop))),
        (
            "sales_recall",
            Cashier,
            Some(|st| sales::recall(st, NO_ID.into()).map(drop)),
        ),
        (
            "sales_discard_held",
            permission,
            Some(|st| sales::discard(st, NO_ID.into())),
        ),
        (
            "sales_record_event",
            permission,
            Some(|st| {
                let event = CartEventInput::CartCancelled {
                    cart: Cart::new(Lang::Fr, vec![]),
                };
                sales::record_event(st, event)
            }),
        ),
        (
            "returns_returnable",
            Cashier,
            Some(|st| returns::returnable(st, NO_ID.into()).map(drop)),
        ),
        (
            "returns_quote",
            permission,
            Some(|st| returns::quote(st, probe_return()).map(drop)),
        ),
        (
            "returns_create",
            permission,
            Some(|st| {
                let request = returns::ReturnRequest {
                    input: probe_return(),
                    print_receipt: false,
                    extras: Default::default(),
                };
                returns::create(st, request).map(drop)
            }),
        ),
        (
            "returns_exchange",
            permission,
            Some(|st| {
                let request = returns::ExchangeRequest {
                    ret: probe_return(),
                    sale: probe_sale(),
                    print_receipt: false,
                    return_extras: Default::default(),
                    sale_extras: Default::default(),
                };
                returns::exchange(st, request).map(drop)
            }),
        ),
        (
            "customers_search",
            Cashier,
            Some(|st| customers::search(st, CustomerQuery::default()).map(drop)),
        ),
        (
            "customers_get",
            Cashier,
            Some(|st| customers::get(st, NO_ID.into()).map(drop)),
        ),
        (
            "customers_find_by_phone",
            Cashier,
            Some(|st| customers::find_by_phone(st, "0550000000".into()).map(drop)),
        ),
        (
            "customers_create",
            permission,
            Some(|st| customers::create(st, CustomerInput::default()).map(drop)),
        ),
        (
            "customers_quick_create",
            Cashier,
            Some(|st| customers::quick_create(st, String::new(), None).map(drop)),
        ),
        (
            "customers_update",
            permission,
            Some(|st| customers::update(st, NO_ID.into(), CustomerInput::default()).map(drop)),
        ),
        (
            "customers_set_blocked",
            permission,
            Some(|st| customers::set_blocked(st, NO_ID.into(), true).map(drop)),
        ),
        (
            "customers_archive",
            permission,
            Some(|st| customers::archive(st, NO_ID.into())),
        ),
        (
            "customers_restore",
            permission,
            Some(|st| customers::restore(st, NO_ID.into())),
        ),
        (
            "customers_documents",
            Cashier,
            Some(|st| customers::documents_of(st, NO_ID.into(), 10).map(drop)),
        ),
        (
            "credit_quote",
            Cashier,
            Some(|st| credit::quote(st, probe_versement()).map(drop)),
        ),
        (
            "credit_collect",
            Cashier,
            Some(|st| {
                let request = credit::CollectRequest {
                    input: probe_versement(),
                    print_receipt: false,
                };
                credit::collect(st, request).map(drop)
            }),
        ),
        (
            "credit_reprint_receipt",
            Cashier,
            Some(|st| credit::reprint_receipt(st, NO_ID.into(), None).map(drop)),
        ),
        (
            "credit_set_opening",
            permission,
            Some(|st| credit::set_opening(st, NO_ID.into(), Money(100), None).map(drop)),
        ),
        (
            "credit_adjust",
            permission,
            Some(|st| credit::adjust(st, NO_ID.into(), Money(100), "Sonde".into()).map(drop)),
        ),
        (
            "credit_entries",
            Cashier,
            Some(|st| credit::entries(st, NO_ID.into(), EntryQuery::default()).map(drop)),
        ),
        (
            "credit_open_items",
            Cashier,
            Some(|st| credit::open_items(st, NO_ID.into()).map(drop)),
        ),
        (
            "credit_statement",
            Cashier,
            Some(|st| {
                credit::statement(st, NO_ID.into(), "2026-01-01".into(), "2026-01-31".into())
                    .map(drop)
            }),
        ),
        (
            "credit_print_statement",
            Cashier,
            Some(|st| {
                let (from, to) = ("2026-01-01".into(), "2026-01-31".into());
                credit::print_statement(st, NO_ID.into(), from, to, Lang::Fr).map(drop)
            }),
        ),
        (
            "credit_aging",
            Cashier,
            Some(|st| credit::aging(st, "2026-01-31".into()).map(drop)),
        ),
        (
            "credit_receivables",
            Cashier,
            Some(|st| credit::receivables(st).map(drop)),
        ),
        (
            "credit_rebuild_balances",
            permission,
            Some(|st| credit::rebuild_balances(st).map(drop)),
        ),
        (
            "credit_verify_receipts",
            permission,
            Some(|st| credit::verify_receipts(st).map(drop)),
        ),
        (
            "suppliers_search",
            permission,
            Some(|st| suppliers::search(st, SupplierQuery::default()).map(drop)),
        ),
        (
            "suppliers_get",
            permission,
            Some(|st| suppliers::get(st, NO_ID.into()).map(drop)),
        ),
        (
            "suppliers_create",
            permission,
            Some(|st| suppliers::create(st, SupplierInput::default()).map(drop)),
        ),
        (
            "suppliers_update",
            permission,
            Some(|st| suppliers::update(st, NO_ID.into(), SupplierInput::default()).map(drop)),
        ),
        (
            "suppliers_archive",
            permission,
            Some(|st| suppliers::archive(st, NO_ID.into())),
        ),
        (
            "suppliers_restore",
            permission,
            Some(|st| suppliers::restore(st, NO_ID.into())),
        ),
        (
            "suppliers_set_opening",
            permission,
            Some(|st| suppliers::set_opening(st, NO_ID.into(), Money(100), None).map(drop)),
        ),
        (
            "suppliers_adjust",
            permission,
            Some(|st| suppliers::adjust(st, NO_ID.into(), Money(100), "Sonde".into()).map(drop)),
        ),
        (
            "suppliers_pay",
            permission,
            Some(|st| {
                let input = SupplierPaymentInput {
                    supplier_id: NO_ID.into(),
                    amount: Money(100),
                    method: PaymentMethod::Transfer,
                    reference: Some("Sonde".into()),
                    cheque_bank: None,
                    cheque_date: None,
                    from_till: false,
                    allocate_to: vec![],
                    note: None,
                };
                suppliers::pay(st, input).map(drop)
            }),
        ),
        (
            "suppliers_entries",
            permission,
            Some(|st| suppliers::entries(st, NO_ID.into(), EntryQuery::default()).map(drop)),
        ),
        (
            "suppliers_open_items",
            permission,
            Some(|st| suppliers::open_items(st, NO_ID.into()).map(drop)),
        ),
        (
            "suppliers_statement",
            permission,
            Some(|st| {
                suppliers::statement(st, NO_ID.into(), "2026-01-01".into(), "2026-01-31".into())
                    .map(drop)
            }),
        ),
        (
            "suppliers_aging",
            permission,
            Some(|st| suppliers::aging(st, "2026-01-31".into()).map(drop)),
        ),
        (
            "suppliers_payables",
            permission,
            Some(|st| suppliers::payables(st).map(drop)),
        ),
        (
            "purchases_list",
            permission,
            Some(|st| purchases::list(st, PurchaseQuery::default()).map(drop)),
        ),
        (
            "purchases_get",
            permission,
            Some(|st| purchases::get(st, NO_ID.into()).map(drop)),
        ),
        (
            "purchases_remaining",
            permission,
            Some(|st| purchases::remaining(st, NO_ID.into()).map(drop)),
        ),
        (
            "purchases_cost_history",
            permission,
            Some(|st| purchases::cost_history(st, NO_ID.into()).map(drop)),
        ),
        (
            "purchases_save_order",
            permission,
            Some(|st| {
                let input = OrderInput {
                    supplier_id: NO_ID.into(),
                    lines: vec![],
                    supplier_ref: None,
                    note: None,
                };
                purchases::save_order(st, None, input).map(drop)
            }),
        ),
        (
            "purchases_validate_order",
            permission,
            Some(|st| purchases::validate_order(st, NO_ID.into()).map(drop)),
        ),
        (
            "purchases_cancel_order",
            permission,
            Some(|st| purchases::cancel_order(st, NO_ID.into())),
        ),
        (
            "purchases_receive",
            permission,
            Some(|st| {
                let input = ReceiptInput {
                    supplier_id: NO_ID.into(),
                    order_id: None,
                    location_id: None,
                    lines: vec![],
                    supplier_ref: None,
                    note: None,
                    invoice: None,
                };
                purchases::receive(st, input).map(drop)
            }),
        ),
        (
            "purchases_record_invoice",
            permission,
            Some(|st| {
                let input = InvoiceInput {
                    supplier_id: NO_ID.into(),
                    supplier_number: "F-0".into(),
                    receipt_ids: vec![],
                    lines: vec![],
                    note: None,
                    payment: None,
                };
                purchases::record_invoice(st, input).map(drop)
            }),
        ),
        (
            "purchases_return",
            permission,
            Some(|st| {
                let input = SupplierReturnInput {
                    supplier_id: NO_ID.into(),
                    location_id: None,
                    lines: vec![],
                    credit_supplier: false,
                    note: None,
                };
                purchases::return_to_supplier(st, input).map(drop)
            }),
        ),
        (
            "lots_of_product",
            Cashier,
            Some(|st| stock_m2::lots_of(st, NO_ID.into()).map(drop)),
        ),
        (
            "lots_expiry",
            Cashier,
            Some(|st| stock_m2::expiry(st, Some(30)).map(drop)),
        ),
        (
            "serials_register",
            permission,
            Some(|st| stock_m2::register_serials(st, NO_ID.into(), vec![], None).map(drop)),
        ),
        (
            "serials_list",
            Cashier,
            Some(|st| stock_m2::list_serials(st, SerialQuery::default()).map(drop)),
        ),
        (
            "serials_find",
            Cashier,
            Some(|st| stock_m2::find_serials(st, "SONDE".into()).map(drop)),
        ),
        (
            "serials_link_sale",
            Cashier,
            Some(|st| stock_m2::link_sale(st, NO_ID.into(), vec![]).map(drop)),
        ),
        (
            "counts_list",
            Cashier,
            Some(|st| stock_m2::list_counts(st, None).map(drop)),
        ),
        (
            "counts_start",
            permission,
            Some(|st| {
                let input = CountInput {
                    location_id: Some(NO_ID.into()),
                    scope: CountScope::Full,
                    category_id: None,
                    product_ids: vec![],
                    blind: true,
                    note: None,
                };
                stock_m2::start_count(st, input).map(drop)
            }),
        ),
        (
            "counts_lines",
            permission,
            Some(|st| stock_m2::count_lines(st, NO_ID.into(), false).map(drop)),
        ),
        (
            "counts_set",
            permission,
            Some(|st| {
                stock_m2::set_counted(st, NO_ID.into(), NO_ID.into(), Qty(1_000), CountMode::Set)
                    .map(drop)
            }),
        ),
        (
            "counts_lot_lines",
            permission,
            Some(|st| stock_m2::count_lot_lines(st, NO_ID.into()).map(drop)),
        ),
        (
            "counts_set_lot",
            permission,
            Some(|st| {
                stock_m2::set_counted_lot(
                    st,
                    NO_ID.into(),
                    NO_ID.into(),
                    "SONDE".into(),
                    Qty(1_000),
                    CountMode::Set,
                )
                .map(drop)
            }),
        ),
        (
            "counts_scan",
            permission,
            Some(|st| stock_m2::scan_count(st, NO_ID.into(), "SONDE".into()).map(drop)),
        ),
        (
            "counts_close",
            permission,
            Some(|st| stock_m2::close_count(st, NO_ID.into(), Uncounted::Ignore).map(drop)),
        ),
        (
            "counts_cancel",
            permission,
            Some(|st| stock_m2::cancel_count(st, NO_ID.into()).map(drop)),
        ),
        (
            "stock_transfer",
            permission,
            Some(|st| {
                let input = TransferInput {
                    from_location_id: NO_ID.into(),
                    to_location_id: NO_ID.into(),
                    lines: vec![],
                    driver_name: None,
                    vehicle: None,
                    note: None,
                };
                stock_m2::transfer(st, input).map(drop)
            }),
        ),
        (
            "stock_transfers",
            Cashier,
            Some(|st| stock_m2::transfers_list(st, 10).map(drop)),
        ),
        (
            "stock_adjust_batch",
            permission,
            Some(|st| {
                let input = AdjustBatchInput {
                    location_id: None,
                    reason: AdjustReason::Correction,
                    lines: vec![],
                    note: None,
                };
                stock_m2::adjust_batch(st, input).map(drop)
            }),
        ),
        (
            "stock_minimums_get",
            Cashier,
            Some(|st| stock_m2::minimums_get(st, NO_ID.into()).map(drop)),
        ),
        (
            "stock_minimums_set",
            permission,
            Some(|st| stock_m2::minimums_set(st, NO_ID.into(), vec![]).map(drop)),
        ),
        (
            "stock_levels_view",
            Cashier,
            Some(|st| stock_m2::levels_view(st, LevelQuery::default()).map(drop)),
        ),
        (
            "stock_movement_log",
            Cashier,
            Some(|st| stock_m2::movement_log(st, MovementQuery::default()).map(drop)),
        ),
        (
            "products_extras_get",
            Cashier,
            Some(|st| product_extras::get_extras(st, NO_ID.into()).map(drop)),
        ),
        (
            "products_extras_set",
            permission,
            Some(|st| {
                product_extras::set_extras(st, NO_ID.into(), ProductExtrasInput::default())
                    .map(drop)
            }),
        ),
        (
            "products_breaks_list",
            Cashier,
            Some(|st| product_extras::list_breaks(st, NO_ID.into()).map(drop)),
        ),
        (
            "products_breaks_replace",
            permission,
            Some(|st| product_extras::replace_breaks(st, NO_ID.into(), vec![]).map(drop)),
        ),
        (
            "products_next_ean13",
            permission,
            Some(|st| product_extras::next_ean13(st).map(drop)),
        ),
        (
            "products_next_reference",
            permission,
            Some(|st| product_extras::next_reference(st).map(drop)),
        ),
        (
            "products_variants_generate",
            permission,
            Some(|st| {
                let input = VariantsInput {
                    axes: vec![],
                    sell_price: None,
                    cost_price: None,
                    internal_barcodes: false,
                };
                product_extras::generate_variants(st, NO_ID.into(), input).map(drop)
            }),
        ),
        (
            "products_bulk_price_preview",
            permission,
            Some(|st| product_extras::bulk_price_preview(st, probe_bulk_price()).map(drop)),
        ),
        (
            "products_bulk_price_apply",
            permission,
            Some(|st| product_extras::bulk_price_apply(st, probe_bulk_price()).map(drop)),
        ),
        (
            "products_bulk_edit",
            permission,
            Some(|st| {
                let edit = BulkEdit {
                    product_id: NO_ID.into(),
                    sell_price: None,
                    cost_price: None,
                    min_stock: None,
                    category_id: None,
                    tax_rate_id: None,
                    available_at_till: None,
                };
                product_extras::bulk_edit(st, vec![edit]).map(drop)
            }),
        ),
        (
            "products_price_help",
            permission,
            Some(|st| {
                let input = PriceHelpInput {
                    cost: Money(100),
                    price: None,
                    target_margin_bp: None,
                    tax_rate_id: NO_ID.into(),
                };
                product_extras::price_help(st, input).map(drop)
            }),
        ),
        // The dialog commands check this before opening their dialog.
        (
            "products_pick_image",
            permission,
            Some(|st| product_extras::may_edit_images(st).map(drop)),
        ),
        (
            "products_clear_image",
            permission,
            Some(|st| {
                let media = std::path::Path::new(&st.paths.media);
                product_extras::clear_image(st, media, NO_ID.into()).map(drop)
            }),
        ),
        (
            "products_import_images",
            permission,
            Some(|st| product_extras::may_edit_images(st).map(drop)),
        ),
        (
            "promotions_list",
            Cashier,
            Some(|st| promotions::list(st, false).map(drop)),
        ),
        (
            "promotions_get",
            Cashier,
            Some(|st| promotions::get(st, NO_ID.into()).map(drop)),
        ),
        (
            "promotions_presets",
            Cashier,
            Some(|st| promotions::presets(st).map(drop)),
        ),
        (
            "promotions_create",
            permission,
            Some(|st| promotions::create(st, probe_promotion()).map(drop)),
        ),
        (
            "promotions_update",
            permission,
            Some(|st| promotions::update(st, NO_ID.into(), probe_promotion()).map(drop)),
        ),
        (
            "promotions_archive",
            permission,
            Some(|st| promotions::archive(st, NO_ID.into())),
        ),
        (
            "promotions_declaration",
            permission,
            Some(|st| promotions::declaration(st, NO_ID.into(), Lang::Fr).map(drop)),
        ),
        (
            "expenses_categories",
            Cashier,
            Some(|st| expenses::categories(st, false).map(drop)),
        ),
        (
            "expenses_category_create",
            permission,
            Some(|st| expenses::create_category(st, probe_expense_category()).map(drop)),
        ),
        (
            "expenses_category_update",
            permission,
            Some(|st| {
                expenses::update_category(st, NO_ID.into(), probe_expense_category()).map(drop)
            }),
        ),
        (
            "expenses_category_archive",
            permission,
            Some(|st| expenses::archive_category(st, NO_ID.into())),
        ),
        (
            "expenses_record",
            Cashier,
            Some(|st| {
                let input = NewExpense {
                    category_id: NO_ID.into(),
                    amount: Money(100),
                    method: PaymentMethod::Cash,
                    from_till: true,
                    reference: None,
                    supplier_id: None,
                    business_date: None,
                    note: None,
                };
                expenses::record(st, input).map(drop)
            }),
        ),
        (
            "expenses_reverse",
            Cashier,
            Some(|st| expenses::reverse(st, NO_ID.into(), "Sonde".into()).map(drop)),
        ),
        (
            "expenses_list",
            permission,
            Some(|st| expenses::list(st, ExpenseQuery::default()).map(drop)),
        ),
        (
            "expenses_totals",
            permission,
            Some(|st| expenses::totals(st, "2026-01-01".into(), "2026-01-31".into()).map(drop)),
        ),
        // The dialog commands check this before opening their dialog.
        (
            "imports_open_file",
            permission,
            Some(|st| imports::may_import(st).map(drop)),
        ),
        (
            "imports_preview",
            permission,
            Some(|st| {
                imports::preview(st, NO_ID.into(), 0, ImportEntity::Products, None).map(drop)
            }),
        ),
        (
            "imports_dry_run",
            permission,
            Some(|st| imports::dry_run(st, NO_ID.into(), 0, probe_import()).map(drop)),
        ),
        (
            "imports_apply",
            permission,
            Some(|st| imports::apply(st, NO_ID.into(), 0, probe_import(), &mut |_| {}).map(drop)),
        ),
        (
            "imports_error_report",
            permission,
            Some(|st| {
                imports::error_report_bytes(st, ImportEntity::Products, &[], Lang::Fr).map(drop)
            }),
        ),
        (
            "imports_template",
            permission,
            Some(|st| imports::template_bytes(st, ImportEntity::Products, Lang::Fr).map(drop)),
        ),
        (
            "imports_jobs",
            permission,
            Some(|st| imports::jobs(st).map(drop)),
        ),
        (
            "imports_undo",
            permission,
            Some(|st| imports::undo(st, NO_ID.into()).map(drop)),
        ),
        (
            "imports_discard",
            permission,
            Some(|st| imports::discard(st, NO_ID.into())),
        ),
        // Each target checks its own permission; the customer file needs customers.manage.
        (
            "exports_save",
            permission,
            Some(|st| {
                let target = exports::ExportTarget::Customers {
                    query: CustomerQuery::default(),
                };
                exports::may_export(st, &target).map(drop)
            }),
        ),
        (
            "reports_catalog",
            Session,
            Some(|st| reports::catalog(st, Lang::Fr).map(drop)),
        ),
        (
            "reports_run",
            permission,
            Some(|st| {
                let request = pos_core::reports::ReportRequest::new(
                    pos_core::reports::ReportKind::SalesSummary,
                    "2026-01-01",
                    "2026-01-31",
                );
                reports::run(st, request).map(drop)
            }),
        ),
        (
            "reports_dashboard",
            permission,
            Some(|st| reports::dashboard(st).map(drop)),
        ),
        (
            "backup_export_file",
            permission,
            Some(|st| data_files::may_manage_data(st).map(drop)),
        ),
        (
            "backup_import_file",
            permission,
            Some(|st| data_files::may_manage_data(st).map(drop)),
        ),
        (
            "data_export_all",
            permission,
            Some(|st| data_files::may_manage_data(st).map(drop)),
        ),
        // The cashier holds `sales.reprint`, `credit.view` and `products.view`: the print,
        // statement, warranty and table probes reach the lookup (`document_not_found`,
        // `customer_not_found`) or the host (`print_host_unavailable`), never a refusal.
        (
            "documents_print",
            Cashier,
            Some(|st| print::print_document(st, &no_host(), probe_print()).map(drop)),
        ),
        (
            "documents_export_pdf",
            Cashier,
            Some(|st| {
                print::export_pdf_to(st, &no_host(), probe_pdf(), Path::new("unused.pdf")).map(drop)
            }),
        ),
        (
            "documents_print_model",
            Cashier,
            Some(|st| print::print_model(st, probe_pdf()).map(drop)),
        ),
        (
            "statements_print",
            Cashier,
            Some(|st| print::print_statement(st, &no_host(), probe_statement()).map(drop)),
        ),
        (
            "statements_export_pdf",
            Cashier,
            Some(|st| {
                let unused = Path::new("unused.pdf");
                print::statement_pdf_to(st, &no_host(), probe_statement(), unused).map(drop)
            }),
        ),
        (
            "warranty_print",
            Cashier,
            Some(|st| print::print_warranty(st, &no_host(), probe_warranty()).map(drop)),
        ),
        (
            "warranty_export_pdf",
            Cashier,
            Some(|st| {
                let unused = Path::new("unused.pdf");
                print::warranty_pdf_to(st, &no_host(), probe_warranty(), unused).map(drop)
            }),
        ),
        (
            "exports_pdf",
            Cashier,
            Some(|st| {
                let request = print::PdfExportRequest {
                    target: exports::ExportTarget::Products,
                    lang: Lang::Fr,
                };
                let unused = Path::new("unused.pdf");
                print::export_table_pdf_to(st, &no_host(), request, unused).map(drop)
            }),
        ),
        (
            "fiscal_g50",
            permission,
            Some(|st| fiscal::g50_of(st, "2026-09".into()).map(drop)),
        ),
        // A backwards period stops the probe at `archive_range_invalid`, before any file.
        (
            "fiscal_export_archive",
            permission,
            Some(|st| {
                let backwards = fiscal::ArchiveInput {
                    from: "2026-12-31".into(),
                    to: "2026-01-01".into(),
                    lang: Lang::Fr,
                };
                let unused = Path::new("unused.zip");
                fiscal::export_archive_to(st, &no_host(), backwards, unused, |_| {}).map(drop)
            }),
        ),
        (
            "fiscal_verify_archive",
            permission,
            Some(|st| fiscal::verify_archive_at(st, Path::new("missing.zip")).map(drop)),
        ),
        // No session: the print host page is not signed in; the window label guards them
        // (`commands::print_host::tests::print_host_commands_refuse_other_windows`).
        ("print_host_next", Public, None),
        ("print_host_rendered", Public, None),
        (
            "labels_templates",
            Cashier,
            Some(|st| labels::templates(st).map(drop)),
        ),
        (
            "labels_save_templates",
            permission,
            Some(|st| {
                labels::save_templates(st, vec![], std::collections::BTreeMap::new()).map(drop)
            }),
        ),
        (
            "labels_build",
            permission,
            Some(|st| labels::build(st, probe_labels()).map(drop)),
        ),
        (
            "labels_preview",
            permission,
            Some(|st| {
                let input = labels::LabelPreviewInput {
                    template: pos_core::labels::builtin_templates().remove(0),
                    product_id: Some(NO_ID.into()),
                    lang: None,
                };
                labels::preview(st, input).map(drop)
            }),
        ),
        (
            "labels_print",
            permission,
            Some(|st| {
                let input = labels::LabelPrintInput {
                    request: probe_labels(),
                    printer_id: None,
                    assign_missing_barcodes: false,
                    arabic_override: false,
                };
                labels::print(st, input).map(drop)
            }),
        ),
        // A weighed label is printed at the counter too.
        (
            "labels_print_weighed",
            Cashier,
            Some(|st| {
                let input = labels::WeighedLabelInput {
                    request: pos_core::labels::WeighedLabelRequest {
                        product_id: NO_ID.into(),
                        net: Qty(1_000),
                        copies: 1,
                        template_id: None,
                        lang: None,
                    },
                    printer_id: None,
                    arabic_override: false,
                };
                labels::print_weighed(st, input).map(drop)
            }),
        ),
        (
            "labels_stale",
            permission,
            Some(|st| labels::stale(st, Some(10)).map(drop)),
        ),
        (
            "labels_dismiss",
            permission,
            Some(|st| labels::dismiss(st, vec![NO_ID.into()]).map(drop)),
        ),
        (
            "labels_sheet",
            permission,
            Some(|st| label_sheets::sheet(st, probe_sheet()).map(drop)),
        ),
        (
            "labels_sheet_print",
            permission,
            Some(|st| label_sheets::print(st, &no_host(), probe_sheet()).map(drop)),
        ),
        // The sheet is made, so the permission checked, before the dialog opens.
        (
            "labels_sheet_export_pdf",
            permission,
            Some(|st| {
                label_sheets::export_pdf(st, &no_host(), probe_sheet(), |_| Ok(None)).map(drop)
            }),
        ),
        (
            "scale_items_list",
            Cashier,
            Some(|st| labels::scale_items(st).map(drop)),
        ),
        (
            "scale_items_set",
            permission,
            Some(|st| labels::set_scale_item(st, NO_ID.into(), Default::default()).map(drop)),
        ),
        // The file is made, so the permission checked, before the dialog opens.
        (
            "scale_plu_export",
            Cashier,
            Some(|st| labels::plu_bytes(st, pos_docs::sheet::SheetFormat::Csv, Lang::Fr).map(drop)),
        ),
        // The dialog command checks this before opening its dialog.
        (
            "counts_import_collector",
            permission,
            Some(labels::may_import_collector),
        ),
        (
            "devices_status",
            Session,
            Some(|st| devices::status(st).map(drop)),
        ),
        // The till weighs; the settings test the scale too.
        (
            "scale_latest",
            Cashier,
            Some(|st| devices::latest(st).map(drop)),
        ),
        (
            "scale_tare",
            Cashier,
            Some(|st| devices::tare(st).map(drop)),
        ),
        (
            "scale_clear_tare",
            Cashier,
            Some(|st| devices::clear_tare(st).map(drop)),
        ),
        ("scale_zero", Cashier, Some(devices::zero)),
        (
            "scale_read_stable",
            Cashier,
            Some(|st| devices::read_stable(st, 100).map(drop)),
        ),
        // No port name: nothing is opened.
        (
            "scale_probe",
            permission,
            Some(|st| devices::probe(st, String::new()).map(drop)),
        ),
        (
            "display_show",
            Cashier,
            Some(|st| devices::show(st, pos_hw::display::DisplayFrame::idle(Lang::Fr))),
        ),
        // The customer screen has no session.
        ("display_current", Public, None),
        ("display_shop", Public, None),
        // No port name: refused before any port opens.
        (
            "display_test_vfd",
            permission,
            Some(|st| devices::test_vfd(st, pos_hw::display::VfdConfig::new(""))),
        ),
        // The window commands check this before touching the customer screen.
        (
            "display_monitors",
            permission,
            Some(devices::may_manage_screen),
        ),
        (
            "display_open_preview",
            permission,
            Some(devices::may_manage_screen),
        ),
        (
            "display_close",
            permission,
            Some(devices::may_manage_screen),
        ),
        (
            "hardware_printer_status",
            Cashier,
            Some(|st| devices::printer_status(st, NO_ID.into()).map(drop)),
        ),
        (
            "hardware_doctor",
            permission,
            Some(|st| devices::doctor(st).map(drop)),
        ),
        (
            "hardware_test_label",
            permission,
            Some(|st| devices::test_label(st, probe_label_printer(), Lang::Fr).map(drop)),
        ),
        // M7 DZBuild link: the probes' state has no DZBuild runtime and no link, so a probe that
        // passes the guard stops at `cloud_config_invalid`, `cloud_not_linked` or an unknown id.
        (
            "cloud_status",
            Session,
            Some(|st| cloud::status(st).map(drop)),
        ),
        (
            "cloud_link_start",
            Owner,
            Some(|st| cloud::link_start(st, Lang::Fr).map(drop)),
        ),
        (
            "cloud_device_flow_start",
            Owner,
            Some(|st| cloud::device_flow_start(st).map(drop)),
        ),
        (
            "cloud_link_poll",
            Owner,
            Some(|st| cloud::link_poll(st).map(drop)),
        ),
        (
            "cloud_link_choose",
            Owner,
            Some(|st| cloud::link_choose(st, "141".into()).map(drop)),
        ),
        ("cloud_link_cancel", Owner, Some(cloud::link_cancel)),
        (
            "cloud_unlink",
            Owner,
            Some(|st| cloud::unlink(st, "0000".into()).map(drop)),
        ),
        ("cloud_sync_now", Cashier, Some(cloud::sync_now)),
        (
            "cloud_notices",
            permission,
            Some(|st| cloud::notices(st).map(drop)),
        ),
        (
            "cloud_notice_dismiss",
            permission,
            Some(|st| cloud::notice_dismiss(st, NO_ID.into())),
        ),
        (
            "cloud_outbox_failed",
            permission,
            Some(|st| cloud::outbox_failed(st).map(drop)),
        ),
        (
            "cloud_outbox_retry",
            permission,
            Some(|st| cloud::outbox_retry(st, NO_ID.into())),
        ),
        (
            "cloud_publish_list",
            permission,
            Some(|st| {
                let query = pos_core::cloud::publish::PublishQuery {
                    text: None,
                    only_published: true,
                    limit: Some(1),
                };
                cloud::publish_list(st, query).map(drop)
            }),
        ),
        (
            "cloud_publish_set",
            permission,
            Some(|st| {
                let change = cloud::PublishChange {
                    product_ids: vec![NO_ID.into()],
                    publish: true,
                    draft: false,
                };
                cloud::publish_set(st, change).map(drop)
            }),
        ),
        (
            "cloud_stock_reconcile",
            permission,
            Some(|st| cloud::stock_reconcile(st, Some(Vec::new())).map(drop)),
        ),
        (
            "cloud_import_online",
            Owner,
            Some(|st| cloud::import_online(st).map(drop)),
        ),
        (
            "cloud_devices",
            permission,
            Some(|st| cloud::devices(st).map(drop)),
        ),
        (
            "cloud_open_page",
            Session,
            Some(|st| cloud::open_page(st, cloud::CloudPage::Store)),
        ),
        // Online orders (no order, no link).
        (
            "cloud_orders_list",
            Cashier,
            Some(|st| {
                let query = pos_core::cloud::orders::OrderQuery {
                    filter: pos_core::cloud::orders::OrderFilter::Open,
                    limit: Some(1),
                };
                cloud_orders::list(st, query).map(drop)
            }),
        ),
        (
            "cloud_orders_get",
            Cashier,
            Some(|st| cloud_orders::get(st, NO_ID.into()).map(drop)),
        ),
        (
            "cloud_orders_seen",
            Cashier,
            Some(|st| cloud_orders::seen(st, vec![NO_ID.into()]).map(drop)),
        ),
        (
            "cloud_orders_claim",
            Cashier,
            Some(|st| cloud_orders::claim(st, NO_ID.into()).map(drop)),
        ),
        (
            "cloud_orders_release",
            Cashier,
            Some(|st| cloud_orders::release(st, NO_ID.into()).map(drop)),
        ),
        (
            "cloud_orders_prepare",
            Cashier,
            Some(|st| cloud_orders::prepare(st, NO_ID.into()).map(drop)),
        ),
        (
            "cloud_orders_quote",
            Cashier,
            Some(|st| {
                cloud_orders::quote(st, NO_ID.into(), "Livraison".into(), Vec::new()).map(drop)
            }),
        ),
        (
            "cloud_orders_sell",
            Cashier,
            Some(|st| {
                let request = cloud_orders::OrderSaleRequest {
                    id: NO_ID.into(),
                    shipping_label: "Livraison".into(),
                    payments: Vec::new(),
                    print_receipt: false,
                    extras: Default::default(),
                };
                cloud_orders::sell(st, request).map(drop)
            }),
        ),
        (
            "cloud_orders_hand_over",
            Cashier,
            Some(|st| {
                cloud_orders::hand_over(st, NO_ID.into(), "Livraison".into(), false, false)
                    .map(drop)
            }),
        ),
        (
            "cloud_orders_ship",
            permission,
            Some(|st| cloud_orders::ship(st, NO_ID.into(), "Livraison".into(), None).map(drop)),
        ),
        (
            "cloud_orders_refuse",
            Cashier,
            Some(|st| {
                let reason = pos_core::cloud::orders::RefuseReason::Other;
                cloud_orders::refuse(st, NO_ID.into(), reason, None).map(drop)
            }),
        ),
        (
            "cloud_customer_flag",
            Cashier,
            Some(|st| cloud_orders::customer_flag(st, "0550123456".into()).map(drop)),
        ),
        ("updates_status", Session, Some(updates::status_guard)),
        (
            "updates_check",
            permission,
            Some(|st| st.require(Permission::SettingsData).map(drop)),
        ),
        (
            "updates_download",
            permission,
            Some(|st| st.require(Permission::SettingsData).map(drop)),
        ),
        (
            "updates_install",
            permission,
            Some(|st| st.require(Permission::SettingsData).map(drop)),
        ),
        (
            "diagnostics_info",
            permission,
            Some(|st| diagnostics::info(st, None).map(drop)),
        ),
        (
            "diagnostics_export",
            permission,
            Some(diagnostics::may_export),
        ),
        (
            "diagnostics_set_log_level",
            permission,
            Some(|st| diagnostics::set_log_level(st, pos_core::local::LogLevel::Info).map(drop)),
        ),
        // The login screen and the wizard apply this PC's display choices before any session.
        ("display_get", Public, None),
        (
            "display_set",
            permission,
            Some(|st| {
                let prefs = display::DisplayPrefs {
                    light_mode: false,
                    text_size: pos_core::local::TextSize::Normal,
                    onscreen_keyboard: false,
                };
                display::set(st, prefs).map(drop)
            }),
        ),
        (
            "app_last_crash",
            Session,
            Some(|st| diagnostics::crash(st).map(drop)),
        ),
        (
            "support_contact",
            Session,
            Some(|st| diagnostics::contact(st).map(drop)),
        ),
        (
            "support_open",
            Session,
            Some(|st| diagnostics::contact(st).map(drop)),
        ),
        // Their own rules (a new install, an owner, the demo profile) are pinned by
        // `demo_tests.rs`.
        ("demo_status", Public, None),
        ("demo_enter", Public, None),
        ("demo_leave", Public, None),
        ("demo_erase", Public, None),
        ("setup_import_backup", Public, None),
        // M8 Task 20: phones and flexy, deposits and service tickets, instalments, wholesale and
        // delivery rounds, orders and appointments. Every module is off in the probes' shop, so a
        // probe past its guard stops at `module_disabled` (or a lookup) and writes nothing.
        (
            "serials_available",
            Cashier,
            Some(|st| phones::serials_available_in(st, NO_ID.into()).map(drop)),
        ),
        (
            "flexy_operators",
            Cashier,
            Some(|st| phones::operators(st).map(drop)),
        ),
        (
            "flexy_line",
            Cashier,
            Some(|st| {
                phones::line(st, "mobilis".into(), "0661234567".into(), Money(100)).map(drop)
            }),
        ),
        (
            "flexy_purchase",
            Cashier,
            Some(|st| phones::purchase(st, probe_float()).map(drop)),
        ),
        (
            "flexy_balances",
            Cashier,
            Some(|st| phones::balances(st).map(drop)),
        ),
        (
            "flexy_entries",
            permission,
            Some(|st| {
                phones::entries(st, None, "2026-09-28".into(), "2026-09-28".into()).map(drop)
            }),
        ),
        (
            "deposits_quote",
            Cashier,
            Some(|st| services::deposit_quote(st, probe_deposit()).map(drop)),
        ),
        (
            "deposits_take",
            Cashier,
            Some(|st| services::deposit_take(st, probe_deposit()).map(drop)),
        ),
        (
            "deposits_refund",
            permission,
            Some(|st| {
                services::deposit_refund(
                    st,
                    pos_core::deposits::OwnerType::ServiceTicket,
                    NO_ID.into(),
                    Money(100),
                    PaymentMethod::Cash,
                    None,
                )
                .map(drop)
            }),
        ),
        (
            "deposits_list",
            Cashier,
            Some(|st| {
                services::deposit_list(
                    st,
                    pos_core::deposits::OwnerType::ServiceTicket,
                    NO_ID.into(),
                )
                .map(drop)
            }),
        ),
        (
            "vouchers_describe",
            Cashier,
            Some(|st| services::voucher_describe(st, "RP2026-000001".into()).map(drop)),
        ),
        (
            "service_intake",
            Cashier,
            Some(|st| services::intake(st, probe_intake()).map(drop)),
        ),
        (
            "service_get",
            Cashier,
            Some(|st| services::get(st, NO_ID.into()).map(drop)),
        ),
        (
            "service_board",
            Cashier,
            Some(|st| services::board(st, Default::default()).map(drop)),
        ),
        (
            "service_set_status",
            Cashier,
            Some(|st| {
                services::set_status(
                    st,
                    NO_ID.into(),
                    pos_core::service_tickets::ServiceStatus::Ready,
                    None,
                )
                .map(drop)
            }),
        ),
        (
            "service_assign",
            Cashier,
            Some(|st| services::assign(st, NO_ID.into(), None).map(drop)),
        ),
        (
            "service_set_items",
            Cashier,
            Some(|st| services::set_items(st, NO_ID.into(), Vec::new()).map(drop)),
        ),
        (
            "service_ready_message",
            Cashier,
            Some(|st| services::ready_message(st, NO_ID.into(), Lang::Fr).map(drop)),
        ),
        (
            "service_pickup",
            Cashier,
            Some(|st| {
                let input = pos_core::service_tickets::PickupInput {
                    id: NO_ID.into(),
                    payments: Vec::new(),
                    extras: Default::default(),
                };
                services::pickup(st, input, false).map(drop)
            }),
        ),
        (
            "service_uncollected",
            Cashier,
            Some(|st| services::uncollected(st).map(drop)),
        ),
        (
            "instalments_quote",
            Cashier,
            Some(|st| instalments::quote(st, probe_instalment_quote()).map(drop)),
        ),
        (
            "instalments_sign",
            Cashier,
            Some(|st| instalments::sign(st, probe_sign(), false).map(drop)),
        ),
        (
            "instalments_contract",
            Cashier,
            Some(|st| instalments::contract(st, NO_ID.into(), Lang::Fr).map(drop)),
        ),
        (
            "instalments_contract_print",
            Cashier,
            Some(|st| {
                instalments::print_contract(st, &no_host(), NO_ID.into(), Lang::Fr).map(drop)
            }),
        ),
        (
            "instalments_get",
            Cashier,
            Some(|st| instalments::get(st, NO_ID.into()).map(drop)),
        ),
        (
            "instalments_list",
            Cashier,
            Some(|st| instalments::list(st, Default::default()).map(drop)),
        ),
        (
            "instalments_collect",
            Cashier,
            Some(|st| {
                instalments::collect(st, NO_ID.into(), probe_plan_payment(), false).map(drop)
            }),
        ),
        (
            "instalments_reminders",
            Cashier,
            Some(|st| instalments::reminders(st, None).map(drop)),
        ),
        (
            "instalments_reminder_text",
            Cashier,
            Some(|st| instalments::reminder_text(st, NO_ID.into(), 1, Lang::Fr).map(drop)),
        ),
        (
            "instalments_note_reminder",
            Cashier,
            Some(|st| instalments::note_reminder(st, NO_ID.into(), 1, None).map(drop)),
        ),
        (
            "instalments_add_guarantee",
            permission,
            Some(|st| {
                let g = pos_core::instalments::GuaranteeInput {
                    kind: pos_core::instalments::GuaranteeKind::IdDocument,
                    holder_name: None,
                    reference: None,
                    phone: None,
                    note: None,
                };
                instalments::add_guarantee(st, NO_ID.into(), g).map(drop)
            }),
        ),
        (
            "wholesale_prices",
            Cashier,
            Some(|st| wholesale::prices(st, NO_ID.into()).map(drop)),
        ),
        (
            "wholesale_price_set",
            permission,
            Some(|st| wholesale::price_set(st, NO_ID.into(), NO_ID.into(), None).map(drop)),
        ),
        // M8 Task 28: the product form reads its minimum order; a round's close is previewed
        (
            "wholesale_min_order",
            Cashier,
            Some(|st| wholesale::min_order(st, NO_ID.into()).map(drop)),
        ),
        (
            "rounds_close_preview",
            Cashier,
            Some(|st| wholesale::round_close_preview(st, NO_ID.into()).map(drop)),
        ),
        (
            "wholesale_min_order_set",
            permission,
            Some(|st| wholesale::min_order_set(st, NO_ID.into(), None).map(drop)),
        ),
        (
            "wholesale_pack_create",
            permission,
            Some(|st| wholesale::pack_create(st, probe_pack()).map(drop)),
        ),
        (
            "rounds_list",
            permission,
            Some(|st| {
                let filter = pos_core::wholesale::rounds::RoundFilter {
                    status: None,
                    from: "2026-09-28".into(),
                    to: "2026-09-28".into(),
                };
                wholesale::rounds(st, filter).map(drop)
            }),
        ),
        (
            "rounds_get",
            permission,
            Some(|st| wholesale::round_get(st, NO_ID.into()).map(drop)),
        ),
        (
            "rounds_create",
            permission,
            Some(|st| wholesale::round_create(st, Default::default()).map(drop)),
        ),
        (
            "rounds_set_stops",
            permission,
            Some(|st| wholesale::round_set_stops(st, NO_ID.into(), Vec::new()).map(drop)),
        ),
        (
            "rounds_start",
            permission,
            Some(|st| wholesale::round_start(st, NO_ID.into()).map(drop)),
        ),
        (
            "rounds_report_stop",
            permission,
            Some(|st| wholesale::round_report_stop(st, NO_ID.into(), probe_stop()).map(drop)),
        ),
        (
            "rounds_close",
            Cashier,
            Some(|st| wholesale::round_close(st, NO_ID.into()).map(drop)),
        ),
        (
            "rounds_route_sheet",
            permission,
            Some(|st| wholesale::route_sheet(st, NO_ID.into(), Lang::Fr).map(drop)),
        ),
        (
            "rounds_route_sheet_print",
            permission,
            Some(|st| {
                wholesale::print_route_sheet(st, &no_host(), NO_ID.into(), Lang::Fr).map(drop)
            }),
        ),
        (
            "rounds_create_van",
            permission,
            Some(|st| {
                let names = wholesale::VanNames {
                    name_ar: String::new(),
                    name_fr: "Camion".into(),
                    name_en: String::new(),
                };
                wholesale::van_create(st, "VAN-1".into(), names).map(drop)
            }),
        ),
        (
            "rounds_load",
            permission,
            Some(|st| wholesale::round_load(st, NO_ID.into(), Vec::new()).map(drop)),
        ),
        (
            "rounds_unload",
            permission,
            Some(|st| wholesale::round_unload(st, NO_ID.into()).map(drop)),
        ),
        (
            "orders_create",
            permission,
            Some(|st| orders::create(st, probe_order()).map(drop)),
        ),
        (
            "orders_get",
            Cashier,
            Some(|st| orders::get(st, NO_ID.into()).map(drop)),
        ),
        (
            "orders_list",
            Cashier,
            Some(|st| orders::list(st, Default::default()).map(drop)),
        ),
        (
            "orders_fulfil_cart",
            Cashier,
            Some(|st| orders::fulfil_cart(st, NO_ID.into()).map(drop)),
        ),
        (
            "orders_pay_in",
            Cashier,
            Some(|st| orders::pay_in(st, NO_ID.into(), probe_order_deposit()).map(drop)),
        ),
        (
            "orders_set_ready",
            Cashier,
            Some(|st| orders::set_ready(st, NO_ID.into(), None).map(drop)),
        ),
        (
            "orders_fulfil",
            Cashier,
            Some(|st| {
                let input = pos_core::orders::FulfilInput {
                    id: NO_ID.into(),
                    payments: Vec::new(),
                    extras: Default::default(),
                };
                orders::fulfil(st, input, false).map(drop)
            }),
        ),
        (
            "orders_deliver",
            permission,
            Some(|st| {
                let input = pos_core::orders::DeliverInput {
                    id: NO_ID.into(),
                    target: DocType::DeliveryNote,
                    payments: Vec::new(),
                };
                orders::deliver(st, input).map(drop)
            }),
        ),
        (
            "orders_close",
            permission,
            Some(|st| orders::close(st, NO_ID.into(), None).map(drop)),
        ),
        (
            "orders_keep_deposit",
            permission,
            Some(|st| orders::keep_deposit(st, NO_ID.into(), None).map(drop)),
        ),
        (
            "appointments_day",
            Cashier,
            Some(|st| orders::appointment_day(st, "2026-09-28".into()).map(drop)),
        ),
        (
            "appointments_range",
            Cashier,
            Some(|st| orders::appointment_range(st, 0, 1, None).map(drop)),
        ),
        (
            "appointments_book",
            Cashier,
            Some(|st| orders::appointment_book(st, probe_appointment()).map(drop)),
        ),
        (
            "appointments_update",
            Cashier,
            Some(|st| orders::appointment_update(st, NO_ID.into(), probe_appointment()).map(drop)),
        ),
        (
            "appointments_set_status",
            Cashier,
            Some(|st| {
                orders::appointment_set_status(
                    st,
                    NO_ID.into(),
                    pos_core::appointments::AppointmentStatus::Done,
                )
                .map(drop)
            }),
        ),
        // M8 Task 21: loyalty and vouchers.
        (
            "loyalty_enrol",
            Cashier,
            Some(|st| loyalty::enrol(st, NO_ID.into(), None).map(drop)),
        ),
        (
            "loyalty_block",
            permission,
            Some(|st| loyalty::block(st, "PROBE000".into()).map(drop)),
        ),
        (
            "loyalty_card_lookup",
            Cashier,
            Some(|st| loyalty::card_lookup(st, "PROBE000".into()).map(drop)),
        ),
        (
            "loyalty_account",
            Cashier,
            Some(|st| loyalty::account(st, NO_ID.into()).map(drop)),
        ),
        (
            "loyalty_entries",
            Cashier,
            Some(|st| loyalty::entries(st, NO_ID.into()).map(drop)),
        ),
        (
            "loyalty_redeem",
            Cashier,
            Some(|st| loyalty::redeem(st, NO_ID.into(), 100).map(drop)),
        ),
        (
            "loyalty_adjust",
            permission,
            Some(|st| loyalty::adjust(st, NO_ID.into(), 10, "probe".into()).map(drop)),
        ),
        (
            "vouchers_quote_sale",
            Cashier,
            Some(|st| loyalty::voucher_quote(st, probe_voucher_sale()).map(drop)),
        ),
        (
            "vouchers_sell",
            Cashier,
            Some(|st| loyalty::voucher_sell(st, probe_voucher_sale()).map(drop)),
        ),
        (
            "vouchers_get",
            Cashier,
            Some(|st| loyalty::voucher_get(st, "BA2026-999999".into()).map(drop)),
        ),
        (
            "vouchers_list",
            permission,
            Some(|st| {
                loyalty::voucher_list(st, pos_core::gift_vouchers::VoucherFilter::default())
                    .map(drop)
            }),
        ),
        (
            "vouchers_reprint",
            Cashier,
            Some(|st| loyalty::voucher_reprint(st, "BA2026-999999".into()).map(drop)),
        ),
        // Jewellery.
        (
            "jewellery_piece_get",
            Cashier,
            Some(|st| jewellery::piece_get(st, NO_ID.into()).map(drop)),
        ),
        (
            "jewellery_piece_set",
            permission,
            Some(|st| jewellery::piece_set(st, NO_ID.into(), probe_piece()).map(drop)),
        ),
        (
            "metal_board",
            Cashier,
            Some(|st| jewellery::board(st).map(drop)),
        ),
        (
            "metal_price_set",
            permission,
            Some(|st| {
                jewellery::set_metal_price(st, pos_core::jewellery::Metal::Gold, 750, Money::ZERO)
                    .map(drop)
            }),
        ),
        (
            "metal_history",
            Cashier,
            Some(|st| jewellery::history(st, pos_core::jewellery::Metal::Gold, 750).map(drop)),
        ),
        (
            "jewellery_reprice",
            permission,
            Some(|st| jewellery::reprice(st).map(drop)),
        ),
        (
            "jeweller_register_entries",
            // the buyers' identity numbers: a manager's (`customers.manage`), never a cashier's
            permission,
            Some(|st| jewellery::register_entries(st, 2026, 1, 10).map(drop)),
        ),
        // Staff and zakat.
        (
            "staff_list",
            permission,
            Some(|st| staff::list(st).map(drop)),
        ),
        (
            "staff_profile_set",
            permission,
            Some(|st| staff::profile_set(st, NO_ID.into(), probe_profile()).map(drop)),
        ),
        (
            "staff_advance",
            Cashier,
            Some(|st| staff::advance(st, probe_advance()).map(drop)),
        ),
        (
            "staff_repay",
            Cashier,
            Some(|st| {
                staff::repay(st, NO_ID.into(), Money::ZERO, PaymentMethod::Cash, None).map(drop)
            }),
        ),
        (
            "staff_deduct",
            permission,
            Some(|st| {
                staff::deduct(st, NO_ID.into(), Money::ZERO, "2026-09".into(), None).map(drop)
            }),
        ),
        (
            "staff_entries",
            permission,
            Some(|st| staff::entries(st, NO_ID.into()).map(drop)),
        ),
        (
            "staff_shifts",
            permission,
            Some(|st| staff::shifts(st, "2026-09-01".into(), "2026-09-30".into(), None).map(drop)),
        ),
        (
            "zakat_preview",
            permission,
            Some(|st| staff::preview_zakat(st, probe_zakat()).map(drop)),
        ),
        (
            "zakat_save",
            permission,
            Some(|st| staff::save_zakat(st, probe_zakat()).map(drop)),
        ),
        (
            "zakat_list",
            permission,
            Some(|st| staff::list_zakat(st).map(drop)),
        ),
        (
            "zakat_statement",
            permission,
            Some(|st| staff::print_zakat(st, &no_host(), NO_ID.into(), Lang::Fr).map(drop)),
        ),
        // Payment types and the MVP completion.
        (
            "payment_types_list",
            Cashier,
            Some(|st| completion::list_types(st, false).map(drop)),
        ),
        (
            "payment_type_save",
            permission,
            Some(|st| completion::save_type(st, None, probe_payment_type()).map(drop)),
        ),
        (
            "payment_type_archive",
            permission,
            Some(|st| completion::archive_type(st, NO_ID.into())),
        ),
        (
            "roles_duplicate",
            Owner,
            Some(|st| {
                let names = completion::RoleNames {
                    name_ar: String::new(),
                    name_fr: "Sonde".into(),
                    name_en: String::new(),
                };
                completion::duplicate_role(st, NO_ID.into(), names).map(drop)
            }),
        ),
        (
            "users_duplicate",
            permission,
            Some(|st| {
                completion::duplicate_user(st, NO_ID.into(), "Probe".into(), "1234".into())
                    .map(drop)
            }),
        ),
        (
            "variant_grid",
            Cashier,
            Some(|st| completion::grid(st, NO_ID.into(), None, None).map(drop)),
        ),
        (
            "payments_baridpay_qr",
            Cashier,
            Some(|st| completion::baridpay_qr(st).map(drop)),
        ),
        // HW-002: a receipt through the Windows driver of a printer that only takes pages.
        (
            "documents_print_receipt_roll",
            Cashier,
            Some(|st| {
                print::print_receipt_roll(st, &no_host(), NO_ID.into(), "Sonde".into(), 80)
                    .map(drop)
            }),
        ),
        // M8 Task 21b: personal data, data packs, the till's key map (core Task 17).
        (
            "privacy_consents",
            Cashier,
            Some(|st| privacy::consents(st, NO_ID.into()).map(drop)),
        ),
        (
            "privacy_set_consent",
            Cashier,
            Some(|st| {
                privacy::set_consent(
                    st,
                    NO_ID.into(),
                    pos_core::privacy::ConsentChannel::Sms,
                    true,
                    pos_core::privacy::ConsentSource::Counter,
                    None,
                )
                .map(drop)
            }),
        ),
        (
            "privacy_export_customer",
            permission,
            Some(|st| privacy::export_customer(st, NO_ID.into(), &std::env::temp_dir()).map(drop)),
        ),
        (
            "privacy_anonymise",
            permission,
            Some(|st| privacy::anonymise(st, NO_ID.into(), None).map(drop)),
        ),
        (
            "privacy_dpo",
            Session,
            Some(|st| privacy::dpo(st).map(drop)),
        ),
        (
            "privacy_register",
            permission,
            Some(|st| privacy::register(st, Lang::Fr).map(drop)),
        ),
        (
            "till_key_map",
            Session,
            Some(|st| privacy::key_map(st).map(drop)),
        ),
        (
            "till_set_key_map",
            permission,
            Some(|st| {
                let map = [("probe".to_owned(), "F1".to_owned())]
                    .into_iter()
                    .collect();
                privacy::set_key_map(st, map).map(drop)
            }),
        ),
        (
            "datapack_installed",
            permission,
            Some(|st| privacy::installed(st).map(drop)),
        ),
        (
            "datapack_install",
            permission,
            Some(|st| {
                privacy::install_files(
                    st,
                    &std::env::temp_dir().join("dzpos-sonde-datapacks"),
                    std::path::Path::new("sonde.json"),
                    std::path::Path::new("sonde.json.sig"),
                    pos_core::datapack_update::DATAPACK_PUBLIC_KEY,
                )
                .map(drop)
            }),
        ),
        // M9 licensing: status, activation and the file are public; removal is the owner's.
        ("license_status", Public, None),
        ("license_activate", Public, None),
        ("license_import_file", Public, None),
        (
            "license_remove",
            Owner,
            Some(|st| license::remove(st, "0000".into()).map(drop)),
        ),
        // M8 (Task 19): activity templates and modules, the superette, restaurant, pharmacy and
        // butcher packs. Reads the till shares need `sales.create`, `products.view` or
        // `stock.view` (the cashier role holds all three).
        ("packs_templates", Public, None),
        (
            "packs_state",
            Session,
            Some(|st| packs::state(st).map(drop)),
        ),
        (
            "packs_apply_template",
            permission,
            Some(|st| packs::apply_template(st, ShopActivity::General, false).map(drop)),
        ),
        (
            "packs_set_modules",
            permission,
            Some(|st| packs::set_modules(st, Vec::new()).map(drop)),
        ),
        (
            "packs_reset_modules",
            permission,
            Some(|st| packs::reset_modules(st).map(drop)),
        ),
        (
            "packs_set_till_mode",
            permission,
            Some(|st| packs::set_till_mode(st, pos_core::packs::TillMode::Retail).map(drop)),
        ),
        (
            "regulated_board",
            Cashier,
            Some(|st| packs::regulated_prices(st, None).map(drop)),
        ),
        (
            "regulated_set",
            permission,
            Some(|st| packs::set_regulation(st, NO_ID.into(), None).map(drop)),
        ),
        (
            "regulated_seed",
            permission,
            Some(|st| packs::seed_regulated(st).map(drop)),
        ),
        (
            "departments_list",
            Cashier,
            Some(|st| packs::department_keys(st, false).map(drop)),
        ),
        (
            "departments_save",
            permission,
            Some(|st| packs::save_department(st, Some(NO_ID.into()), probe_department()).map(drop)),
        ),
        (
            "departments_archive",
            permission,
            Some(|st| packs::archive_department(st, NO_ID.into())),
        ),
        (
            "dining_rooms",
            Cashier,
            Some(|st| restaurant::rooms(st).map(drop)),
        ),
        (
            "dining_room_save",
            permission,
            Some(|st| restaurant::room_save(st, Some(NO_ID.into()), probe_room()).map(drop)),
        ),
        (
            "dining_room_archive",
            permission,
            Some(|st| restaurant::room_archive(st, NO_ID.into())),
        ),
        (
            "dining_tables",
            Cashier,
            Some(|st| restaurant::tables(st, None).map(drop)),
        ),
        (
            "dining_table_save",
            permission,
            Some(|st| restaurant::table_save(st, Some(NO_ID.into()), probe_table()).map(drop)),
        ),
        (
            "dining_table_archive",
            permission,
            Some(|st| restaurant::table_archive(st, NO_ID.into())),
        ),
        (
            "dining_board",
            Cashier,
            Some(|st| restaurant::board(st, None).map(drop)),
        ),
        (
            "dining_counter",
            Cashier,
            Some(|st| restaurant::counter(st).map(drop)),
        ),
        (
            "daily_menu_get",
            Cashier,
            Some(|st| restaurant::menu_get(st, "2026-09-28".into()).map(drop)),
        ),
        (
            "daily_menu_set",
            permission,
            Some(|st| restaurant::menu_set(st, "2026-09-28".into(), vec![NO_ID.into()]).map(drop)),
        ),
        (
            "modifier_groups",
            Cashier,
            Some(|st| restaurant::groups(st).map(drop)),
        ),
        (
            "modifier_group_save",
            permission,
            Some(|st| {
                restaurant::group_save(st, Some(NO_ID.into()), probe_modifier_group()).map(drop)
            }),
        ),
        (
            "modifier_group_archive",
            permission,
            Some(|st| restaurant::group_archive(st, NO_ID.into())),
        ),
        (
            "modifiers_for_product",
            Cashier,
            Some(|st| restaurant::groups_of_product(st, NO_ID.into()).map(drop)),
        ),
        (
            "modifiers_set_product",
            permission,
            Some(|st| restaurant::set_product_groups(st, NO_ID.into(), Vec::new())),
        ),
        (
            "kitchen_stations",
            Cashier,
            Some(|st| restaurant::stations(st).map(drop)),
        ),
        (
            "kitchen_station_save",
            permission,
            Some(|st| restaurant::station_save(st, Some(NO_ID.into()), probe_station()).map(drop)),
        ),
        (
            "kitchen_station_archive",
            permission,
            Some(|st| restaurant::station_archive(st, NO_ID.into())),
        ),
        (
            "dining_ticket_open",
            Cashier,
            Some(|st| restaurant::ticket_open(st, probe_ticket_open()).map(drop)),
        ),
        (
            "dining_ticket_get",
            Cashier,
            Some(|st| restaurant::ticket_get(st, NO_ID.into()).map(drop)),
        ),
        (
            "dining_ticket_add_line",
            Cashier,
            Some(|st| {
                let input = pos_core::restaurant::tickets::AddLineInput {
                    cart_line: CartLine::product(NO_ID, Qty(1_000)),
                    seat: None,
                    course: None,
                    modifiers: Vec::new(),
                };
                restaurant::ticket_add_line(st, NO_ID.into(), input).map(drop)
            }),
        ),
        (
            "dining_ticket_set_qty",
            Cashier,
            Some(|st| restaurant::ticket_set_qty(st, NO_ID.into(), Qty(1_000)).map(drop)),
        ),
        (
            "dining_ticket_remove_unsent",
            Cashier,
            Some(|st| restaurant::ticket_remove_unsent(st, NO_ID.into()).map(drop)),
        ),
        (
            "dining_ticket_edit_line",
            Cashier,
            Some(|st| {
                let edit = pos_core::restaurant::tickets::LineEdit {
                    seat: None,
                    course: None,
                    note: None,
                };
                restaurant::ticket_edit_line(st, NO_ID.into(), edit).map(drop)
            }),
        ),
        (
            "dining_ticket_set_details",
            Cashier,
            Some(|st| {
                let details = pos_core::restaurant::tickets::TicketDetails {
                    covers: 1,
                    note: None,
                    customer_id: None,
                    delivery_address: None,
                    label: None,
                };
                restaurant::ticket_set_details(st, NO_ID.into(), details).map(drop)
            }),
        ),
        (
            "dining_ticket_transfer",
            Cashier,
            Some(|st| restaurant::ticket_transfer(st, NO_ID.into(), NO_ID.into()).map(drop)),
        ),
        (
            "dining_ticket_merge",
            Cashier,
            Some(|st| restaurant::ticket_merge(st, NO_ID.into(), "t2".into()).map(drop)),
        ),
        (
            "dining_ticket_send",
            Cashier,
            Some(|st| restaurant::ticket_send(st, NO_ID.into()).map(drop)),
        ),
        (
            "dining_ticket_void_line",
            permission,
            Some(|st| {
                restaurant::void_line(
                    st,
                    NO_ID.into(),
                    None,
                    pos_core::restaurant::tickets::VoidReason::Mistake,
                )
                .map(drop)
            }),
        ),
        (
            "dining_ticket_void",
            permission,
            Some(|st| {
                restaurant::void_ticket(
                    st,
                    NO_ID.into(),
                    pos_core::restaurant::tickets::VoidReason::Mistake,
                )
                .map(drop)
            }),
        ),
        (
            "dining_ticket_comp_line",
            permission,
            Some(|st| {
                restaurant::comp_line(
                    st,
                    NO_ID.into(),
                    pos_core::restaurant::tickets::CompReason::Offered,
                )
                .map(drop)
            }),
        ),
        (
            "dining_ticket_prebill",
            Cashier,
            Some(|st| restaurant::ticket_prebill(st, NO_ID.into(), None).map(drop)),
        ),
        (
            "dining_ticket_preview",
            Cashier,
            Some(|st| restaurant::ticket_preview(st, NO_ID.into(), Vec::new()).map(drop)),
        ),
        (
            "dining_ticket_split",
            Cashier,
            Some(|st| {
                let by = pos_core::restaurant::bill::SplitBy::Seat { seat: 1 };
                restaurant::ticket_split(st, NO_ID.into(), by).map(drop)
            }),
        ),
        (
            "dining_ticket_pay",
            Cashier,
            Some(|st| {
                let input = pos_core::restaurant::bill::PayInput {
                    ticket_id: NO_ID.into(),
                    payments: Vec::new(),
                };
                restaurant::ticket_pay(st, input, false).map(drop)
            }),
        ),
        (
            "kitchen_reprint",
            Cashier,
            Some(|st| restaurant::reprint_kitchen(st, NO_ID.into())),
        ),
        (
            "pharmacy_product_get",
            Cashier,
            Some(|st| pharmacy::product_get(st, NO_ID.into()).map(drop)),
        ),
        (
            "pharmacy_product_set",
            permission,
            Some(|st| pharmacy::product_set(st, NO_ID.into(), Default::default()).map(drop)),
        ),
        (
            "pharmacy_equivalents",
            Cashier,
            Some(|st| pharmacy::equivalents(st, NO_ID.into(), None).map(drop)),
        ),
        (
            "pharmacy_expiry_board",
            Cashier,
            Some(|st| pharmacy::expiry_board(st, Some(30)).map(drop)),
        ),
        (
            "chifa_check",
            permission,
            Some(|st| pharmacy::check_chifa(st, NO_ID.into(), probe_chifa()).map(drop)),
        ),
        (
            "chifa_apply",
            permission,
            Some(|st| pharmacy::apply_chifa(st, NO_ID.into(), probe_chifa()).map(drop)),
        ),
        (
            "chifa_list",
            Cashier,
            Some(|st| pharmacy::chifa_prices(st, String::new(), 10).map(drop)),
        ),
        (
            "chifa_of_product",
            Cashier,
            Some(|st| pharmacy::chifa_price_of(st, NO_ID.into()).map(drop)),
        ),
        (
            "controlled_register_entries",
            permission,
            Some(|st| {
                pharmacy::register_entries(st, "2026-09-28".into(), "2026-09-28".into()).map(drop)
            }),
        ),
        (
            "yield_templates",
            Cashier,
            Some(|st| butcher::templates(st).map(drop)),
        ),
        (
            "yield_template_save",
            permission,
            Some(|st| {
                butcher::template_save(st, Some(NO_ID.into()), probe_yield_template()).map(drop)
            }),
        ),
        (
            "yield_template_archive",
            permission,
            Some(|st| butcher::template_archive(st, NO_ID.into())),
        ),
        (
            "yield_preview",
            permission,
            Some(|st| butcher::preview(st, probe_yield_run()).map(drop)),
        ),
        (
            "yield_run",
            permission,
            Some(|st| butcher::run(st, probe_yield_run()).map(drop)),
        ),
        (
            "yield_runs",
            Cashier,
            Some(|st| butcher::runs(st, "2026-09-28".into(), "2026-09-28".into()).map(drop)),
        ),
        (
            "waste_record",
            permission,
            Some(|st| {
                let input = pos_core::butcher::waste::WasteInput {
                    location_id: Some(NO_ID.into()),
                    lines: Vec::new(),
                    note: None,
                };
                butcher::record_waste(st, input).map(drop)
            }),
        ),
        (
            "waste_day",
            Cashier,
            Some(|st| butcher::waste_of_day(st, "2026-09-28".into()).map(drop)),
        ),
    ]
}

/// A float bought for an operator nobody sells: the probe buys nothing.
fn probe_float() -> pos_core::flexy::FloatPurchase {
    pos_core::flexy::FloatPurchase {
        operator: "mobilis".into(),
        amount: Money(100),
        cost: Money(96),
        from_till: false,
        note: None,
    }
}

/// A gift voucher of nothing: no shop sells it.
fn probe_voucher_sale() -> pos_core::gift_vouchers::SellInput {
    pos_core::gift_vouchers::SellInput {
        amount: Money::ZERO,
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        customer_id: None,
        buyer_name: None,
    }
}

/// A piece of no weight: no shop saves it.
fn probe_piece() -> pos_core::jewellery::PieceInput {
    pos_core::jewellery::PieceInput {
        metal: pos_core::jewellery::Metal::Gold,
        titre_permille: 750,
        weight_mg: 0,
        facon: Money::ZERO,
        stones: None,
        hallmark: None,
        priced_by_weight: true,
    }
}

/// A profile with a commission above the cap: no shop saves it.
fn probe_profile() -> pos_core::staff::ProfileInput {
    pos_core::staff::ProfileInput {
        phone: None,
        hire_date: None,
        monthly_salary: None,
        commission_bp: -1,
        note: None,
    }
}

/// An advance of nothing to nobody: no shop pays it.
fn probe_advance() -> pos_core::staff::AdvanceInput {
    pos_core::staff::AdvanceInput {
        user_id: NO_ID.into(),
        amount: Money::ZERO,
        method: PaymentMethod::Cash,
        from_till: false,
        note: None,
    }
}

/// A deposit for a service ticket that does not exist: the probe takes nothing.
fn probe_deposit() -> pos_core::deposits::DepositInput {
    pos_core::deposits::DepositInput {
        owner_type: pos_core::deposits::OwnerType::ServiceTicket,
        owner_id: NO_ID.into(),
        amount: Money(100),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        note: None,
    }
}

/// A statement with a negative bank balance: no shop computes it.
fn probe_zakat() -> pos_core::zakat::ZakatInput {
    pos_core::zakat::ZakatInput {
        day: None,
        valuation: pos_core::zakat::Valuation::Cost,
        bank: Money(-1),
        other_assets: Money::ZERO,
        other_debts: Money::ZERO,
        note: None,
    }
}

/// An intake for a customer who does not exist: the probe opens nothing.
fn probe_intake() -> pos_core::service_tickets::IntakeInput {
    pos_core::service_tickets::IntakeInput {
        kind: pos_core::service_tickets::ServiceKind::Repair,
        customer_id: NO_ID.into(),
        item: "Sonde".into(),
        brand: None,
        model: None,
        serial_no: None,
        accessories: None,
        condition: None,
        fault: "Sonde".into(),
        estimate: None,
        promised_on: None,
        technician_id: None,
        lang: Lang::Fr,
        items: Vec::new(),
    }
}

/// An instalment quote of an empty cart for a customer who does not exist.
fn probe_instalment_quote() -> pos_core::instalments::QuoteInput {
    let mut cart = Cart::new(Lang::Fr, vec![]);
    cart.customer_id = Some(NO_ID.into());
    pos_core::instalments::QuoteInput {
        cart,
        down: Money::ZERO,
        down_method: PaymentMethod::Cash,
        count: 1,
        first_due_on: "2099-01-31".into(),
    }
}

fn probe_sign() -> pos_core::instalments::SignInput {
    pos_core::instalments::SignInput {
        quote: probe_instalment_quote(),
        down_reference: None,
        cheque_bank: None,
        cheque_date: None,
        guarantees: Vec::new(),
        extras: Default::default(),
        note: None,
    }
}

/// A payment on a contract that does not exist: the probe collects nothing.
fn probe_plan_payment() -> pos_core::instalments::PlanPayment {
    pos_core::instalments::PlanPayment {
        amount: Money(100),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        note: None,
    }
}

/// A pack of a product that does not exist: the probe creates nothing.
fn probe_pack() -> pos_core::wholesale::PackInput {
    pos_core::wholesale::PackInput {
        unit_product_id: NO_ID.into(),
        units_per_pack: Qty(6_000),
        unit_id: pos_core::catalog::UNIT_CARTON_ID.into(),
        name_ar: String::new(),
        name_fr: "Sonde".into(),
        name_en: String::new(),
        price: Money(100),
        barcode: None,
    }
}

fn probe_stop() -> pos_core::wholesale::rounds::StopReport {
    pos_core::wholesale::rounds::StopReport {
        seq: 1,
        status: pos_core::wholesale::rounds::StopStatus::Failed,
        collected: Money::ZERO,
        method: None,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        note: None,
    }
}

/// An order of an empty cart for a customer who does not exist.
fn probe_order() -> pos_core::orders::CustomerOrderInput {
    let mut cart = Cart::new(Lang::Fr, vec![]);
    cart.customer_id = Some(NO_ID.into());
    pos_core::orders::CustomerOrderInput {
        draft_id: None,
        cart,
        kind: pos_core::orders::OrderKind::Order,
        due_on: None,
        reserve: false,
        deposit: None,
        note: None,
    }
}

fn probe_order_deposit() -> pos_core::orders::OrderDeposit {
    pos_core::orders::OrderDeposit {
        amount: Money(100),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
    }
}

/// An appointment of five minutes in 2099 for a passer-by.
fn probe_appointment() -> pos_core::appointments::AppointmentInput {
    pos_core::appointments::AppointmentInput {
        customer_id: None,
        customer_name: "Sonde".into(),
        phone: None,
        service_product_id: None,
        staff_user_id: None,
        starts_at: 4_070_908_800_000,
        duration_min: 5,
        note: None,
    }
}

/// A payment type without a code or a name: no shop saves it.
fn probe_payment_type() -> pos_core::payment_types::PaymentTypeInput {
    pos_core::payment_types::PaymentTypeInput {
        code: String::new(),
        name_ar: String::new(),
        name_fr: String::new(),
        name_en: String::new(),
        base_method: PaymentMethod::CardCib,
        requires_reference: false,
        sort_order: 0,
    }
}

/// A department key whose tax rate does not exist: no shop saves it.
fn probe_department() -> pos_core::catalog::departments::DepartmentKeyInput {
    pos_core::catalog::departments::DepartmentKeyInput {
        name_ar: String::new(),
        name_fr: "Sonde".into(),
        name_en: String::new(),
        category_id: None,
        tax_rate_id: NO_ID.into(),
        unit_id: None,
        color: None,
        sort_order: 0,
    }
}

/// A room of no cells: no shop saves it.
fn probe_room() -> pos_core::restaurant::floor::RoomInput {
    pos_core::restaurant::floor::RoomInput {
        code: "SONDE".into(),
        name_ar: String::new(),
        name_fr: "Sonde".into(),
        name_en: String::new(),
        grid_cols: 0,
        grid_rows: 0,
        sort_order: 0,
    }
}

/// A table in a room that does not exist.
fn probe_table() -> pos_core::restaurant::floor::TableInput {
    pos_core::restaurant::floor::TableInput {
        room_id: NO_ID.into(),
        code: "T0".into(),
        seats: 2,
        shape: pos_core::restaurant::floor::TableShape::Square,
        grid_col: 0,
        grid_row: 0,
    }
}

/// A group whose minimum exceeds its maximum: no shop saves it.
fn probe_modifier_group() -> pos_core::restaurant::modifiers::ModifierGroupInput {
    pos_core::restaurant::modifiers::ModifierGroupInput {
        name_ar: String::new(),
        name_fr: "Sonde".into(),
        name_en: String::new(),
        min_select: 3,
        max_select: 1,
        sort_order: 0,
        options: Vec::new(),
    }
}

/// A station mapped to a category that does not exist.
fn probe_station() -> pos_core::restaurant::kitchen::StationInput {
    pos_core::restaurant::kitchen::StationInput {
        code: "SONDE".into(),
        name_ar: String::new(),
        name_fr: "Sonde".into(),
        name_en: String::new(),
        sort_order: 0,
        category_ids: vec![NO_ID.into()],
    }
}

/// A delivery ticket on a table that does not exist.
fn probe_ticket_open() -> pos_core::restaurant::tickets::OpenInput {
    pos_core::restaurant::tickets::OpenInput {
        mode: pos_core::restaurant::tickets::DiningMode::Delivery,
        table_id: Some(NO_ID.into()),
        label: None,
        covers: 1,
        customer_id: None,
        delivery_address: None,
        lang: Lang::Fr,
        price_level_id: None,
    }
}

fn probe_chifa() -> pos_core::pharmacy::chifa::ChifaMapping {
    pos_core::pharmacy::chifa::ChifaMapping {
        code: 0,
        name: 1,
        reference_price: 2,
        reimbursable: None,
    }
}

/// A template of a carcass that does not exist.
fn probe_yield_template() -> pos_core::butcher::yields::YieldTemplateInput {
    pos_core::butcher::yields::YieldTemplateInput {
        source_product_id: NO_ID.into(),
        name: "Sonde".into(),
        cuts: Vec::new(),
    }
}

/// A run of a carcass that does not exist into one cut that does not either.
pub(super) fn probe_yield_run() -> pos_core::butcher::yields::RunInput {
    pos_core::butcher::yields::RunInput {
        template_id: None,
        source_product_id: NO_ID.into(),
        location_id: Some(NO_ID.into()),
        input_qty: Qty(1_000),
        input_cost: None,
        method: pos_core::butcher::yields::CostMethod::Weight,
        cuts: vec![pos_core::butcher::yields::RunCut {
            product_id: NO_ID.into(),
            qty: Qty(900),
        }],
        note: None,
    }
}

/// A print host whose surface answers `print_host_unavailable`: a probe that passes the guard
/// stops there, nothing is printed or written.
fn no_host() -> crate::print_host::PrintHost {
    use crate::print_host::{HostJob, PageSetup, PrintStatus, PrintSurface, Rendered};
    struct Off;
    impl PrintSurface for Off {
        fn render(&self, _: HostJob) -> Result<Rendered, pos_core::error::AppError> {
            Err(pos_core::error::AppError::conflict(
                "print_host_unavailable",
            ))
        }
        fn print(
            &self,
            _: &PageSetup,
            _: Option<&str>,
            _: u32,
        ) -> Result<PrintStatus, pos_core::error::AppError> {
            Err(pos_core::error::AppError::conflict(
                "print_host_unavailable",
            ))
        }
        fn pdf(&self, _: &PageSetup, _: &Path) -> Result<(), pos_core::error::AppError> {
            Err(pos_core::error::AppError::conflict(
                "print_host_unavailable",
            ))
        }
        fn close_if_idle(&self, _: std::time::Duration) {}
    }
    crate::print_host::PrintHost::new(
        std::sync::Arc::new(Off),
        std::sync::Arc::new(crate::print_host::HostChannel::new()),
    )
}

/// A print of a document that does not exist.
fn probe_print() -> print::PrintDocumentInput {
    print::PrintDocumentInput {
        id: NO_ID.into(),
        options: PrintOptions::default(),
        printer: None,
        copies: None,
    }
}

/// A PDF (or preview) of a document that does not exist.
fn probe_pdf() -> print::PdfDocumentInput {
    print::PdfDocumentInput {
        id: NO_ID.into(),
        options: PrintOptions::default(),
    }
}

/// The statement of a customer who does not exist.
fn probe_statement() -> print::StatementPrintInput {
    print::StatementPrintInput {
        side: print::StatementSide::Customer,
        party_id: NO_ID.into(),
        from: "2026-01-01".into(),
        to: "2026-12-31".into(),
        lang: Lang::Fr,
    }
}

/// The warranty certificate of a document that does not exist.
fn probe_warranty() -> print::WarrantyInput {
    print::WarrantyInput {
        document_id: NO_ID.into(),
        line_id: None,
        lang: None,
    }
}

/// A label printer with no file: nothing is printed.
fn probe_label_printer() -> pos_hw::config::LabelPrinterConfig {
    pos_hw::config::LabelPrinterConfig {
        id: "l1".into(),
        name: "Sonde".into(),
        transport: Transport::File {
            path: String::new(),
        },
        profile: pos_hw::labels::profile::LabelProfile::preset(
            pos_hw::labels::profile::LabelPreset::Xprinter,
        ),
    }
}

/// Labels of no product: nothing to build or print.
fn probe_labels() -> pos_core::labels::LabelRequest {
    pos_core::labels::LabelRequest {
        source: pos_core::labels::LabelSource::Products { items: vec![] },
        template_id: None,
        lang: None,
    }
}

/// A sheet of no label.
fn probe_sheet() -> label_sheets::SheetInput {
    label_sheets::SheetInput {
        request: probe_labels(),
        format_id: "a4_24_70x37".into(),
        start: 1,
        arabic_override: false,
    }
}

/// Import options no staged file matches (the token does not exist).
fn probe_import() -> pos_core::imports::engine::ImportOptions {
    pos_core::imports::engine::ImportOptions {
        entity: ImportEntity::Products,
        policy: pos_core::imports::spec::ImportPolicy::CreateOnly,
        file_name: "sonde.csv".into(),
        header_row: 0,
        mapping: vec![],
        location_id: None,
        count_id: None,
    }
}

/// A change of the price of a product that does not exist: the probe reprices nothing.
fn probe_bulk_price() -> BulkPriceInput {
    BulkPriceInput {
        product_ids: vec![NO_ID.into()],
        category_id: None,
        brand: None,
        supplier_id: None,
        price_level_id: None,
        change: PriceChange::Percent(Rate(0)),
        round_to: Money::ZERO,
    }
}

/// A promotion without a name: no shop saves it.
fn probe_promotion() -> PromotionInput {
    PromotionInput {
        name_ar: String::new(),
        name_fr: String::new(),
        name_en: String::new(),
        kind: PromoKind::Percent,
        campaign: Campaign::Promotion,
        trigger_mode: TriggerMode::Manual,
        scope: PromoScope::All,
        value: 500,
        buy_qty: None,
        get_qty: None,
        min_qty: Qty::ZERO,
        priority: 0,
        starts_on: None,
        ends_on: None,
        active: true,
    }
}

/// An expense category without a code or a name: no shop saves it.
fn probe_expense_category() -> ExpenseCategoryInput {
    ExpenseCategoryInput {
        code: String::new(),
        name_ar: String::new(),
        name_fr: String::new(),
        name_en: String::new(),
        cash_group: ExpenseCategory::Other,
        sort_order: 0,
    }
}

/// A versement from a customer who does not exist: the probe collects nothing.
fn probe_versement() -> CollectInput {
    CollectInput {
        customer_id: NO_ID.into(),
        amount: Money(100),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        allocate_to: vec![],
        note: None,
    }
}

/// A sale no till accepts (its cart is empty): the probe sells nothing.
fn probe_sale() -> SaleInput {
    SaleInput {
        cart: Cart::new(Lang::Fr, vec![]),
        payments: vec![],
    }
}

/// A return of nothing from a ticket that does not exist: the probe refunds nothing.
fn probe_return() -> ReturnInput {
    ReturnInput {
        original_id: Some(NO_ID.into()),
        lines: vec![],
        refund_method: PaymentMethod::Cash,
        refund_reference: None,
        lang: Lang::Fr,
        note: None,
    }
}

/// Settings no registry saves (the receipt printer does not exist): the probe changes nothing.
fn probe_hardware() -> HardwareConfig {
    HardwareConfig {
        receipt_printer_id: Some(NO_ID.into()),
        ..HardwareConfig::default()
    }
}

/// A printer no registry prints on (its file path is empty).
fn probe_printer() -> PrinterConfig {
    PrinterConfig {
        id: "p1".into(),
        name: "Sonde".into(),
        transport: Transport::File {
            path: String::new(),
        },
        profile: Profile::preset(Preset::Generic, PaperWidth::Mm80),
        mode: pos_hw::config::PrintMode::Raw,
        paper_width_mm: 80,
    }
}

/// A product input no shop can save (its unit and VAT rate do not exist).
fn probe_product() -> ProductInput {
    ProductInput::simple("Sonde", NO_ID, NO_ID, Money::ZERO)
}

/// A quote of a product that does not exist: no shop prices it.
fn probe_draft() -> DraftInput {
    DraftInput {
        doc_type: DocType::Quote,
        cart: Cart::new(Lang::Fr, vec![CartLine::product(NO_ID, Qty(1_000))]),
        valid_until: None,
        due_date: None,
        second_lang: None,
        draft_id: None,
    }
}

/// A credit note on an invoice that does not exist: the probe refunds nothing.
fn probe_credit() -> CreditNoteInput {
    CreditNoteInput {
        invoice_id: NO_ID.into(),
        kind: CreditKind::Goods,
        lines: Vec::new(),
        amount: None,
        reason: CreditReason::Other,
        refund_method: PaymentMethod::Cash,
        refund_reference: None,
        note: None,
        second_lang: None,
    }
}

const REFUSALS: [&str; 3] = ["session_required", "insufficient_permission", "owner_only"];

/// Review finding: a command without a guard (like `app_open_folder` was) fails CI here.
#[test]
fn every_command_pins_its_guard() {
    let pinned: std::collections::BTreeSet<&str> = guards().iter().map(|(n, ..)| *n).collect();
    let listed: std::collections::BTreeSet<&str> = crate::COMMANDS.iter().copied().collect();
    assert_eq!(
        pinned, listed,
        "pin the guard of every command in `guards()`"
    );
    for (name, guard, probe) in guards() {
        assert_eq!(
            probe.is_some(),
            guard != Guard::Public,
            "{name}: guarded commands, and only those, have a probe"
        );
    }

    let st = AppState::for_tests();
    first_run(&st);
    // Every permission, yet not an owner: owner-only commands must still refuse.
    let deputy_role =
        users::create_role(&st, role_input("Adjoint", Permission::ALL.to_vec())).unwrap();
    let deputy = add_user(&st, "Karima", &deputy_role.id, "7391");
    let cashier = add_user(&st, "Yacine", CASHIER_ROLE_ID, "4826");
    auth::logout(&st).unwrap();
    let probes: Vec<(&str, Guard, Probe)> = guards()
        .into_iter()
        .filter_map(|(name, guard, probe)| probe.map(|p| (name, guard, p)))
        .collect();
    for (name, _, probe) in &probes {
        assert_eq!(
            probe(&st).unwrap_err().code(),
            "session_required",
            "{name} without a session"
        );
    }

    switch_to(&st, &cashier.id, "4826");
    for (name, guard, probe) in &probes {
        let code = probe(&st).err().map(|e| e.code());
        if matches!(guard, Guard::Session | Guard::Cashier) {
            assert!(
                code.as_deref().is_none_or(|c| !REFUSALS.contains(&c)),
                "{name} refused a signed-in cashier: {code:?}"
            );
        } else {
            assert_eq!(
                code.as_deref(),
                Some("insufficient_permission"),
                "{name} as a cashier"
            );
        }
    }

    switch_to(&st, &deputy.id, "7391");
    for (name, guard, probe) in &probes {
        let code = probe(&st).err().map(|e| e.code());
        if *guard == Guard::Owner {
            assert_eq!(code.as_deref(), Some("owner_only"), "{name} as a non-owner");
        } else {
            assert!(
                code.as_deref().is_none_or(|c| !REFUSALS.contains(&c)),
                "{name} refused a user holding every permission: {code:?}"
            );
        }
    }
}

#[test]
fn folders_open_without_a_session_only_from_the_fatal_screen() {
    assert!(
        app::may_open_folder(None, true).is_ok(),
        "the database did not open"
    );
    assert_eq!(
        app::may_open_folder(None, false).unwrap_err().code(),
        "session_required"
    );
    let st = AppState::for_tests();
    assert_eq!(
        app::may_open_folder(Some(&st), true).unwrap_err().code(),
        "session_required",
        "an open database always needs the permission"
    );
    first_run(&st);
    app::may_open_folder(Some(&st), false).unwrap();
}

/// `(seq, hash)` of the last audit row.
fn tail_of(db: &pos_core::db::Db) -> (i64, String) {
    db.read(|c| {
        Ok(c.query_row(
            "SELECT seq, hash FROM audit_log ORDER BY seq DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    })
    .unwrap()
}

/// What `restore_and_stop` leaves for the next start.
struct Restored {
    paths: AppPaths,
    owner: Session,
    source: String,
    replaced: (i64, String),
    pre_restore_file: String,
    marker: String,
}

/// An owner restores the backup made right after setup, then the app "restarts" (the state is
/// dropped).
fn restore_and_stop(dir: &std::path::Path) -> Restored {
    let paths = AppPaths::new(dir);
    let st = AppState::open(&paths, crate::printing::silent()).unwrap();
    let owner = first_run(&st);
    let made = backup::backup_now_impl(&st).unwrap();
    auth::set_language(&st, Lang::Ar).unwrap(); // a row the restored copy does not hold
    backup::restore(&st, &made.file_name, "2580").unwrap();
    let replaced = tail_of(&st.db);
    let saved = st
        .db
        .list_backups()
        .unwrap()
        .into_iter()
        .find(|b| b.kind == pos_core::db::BackupKind::PreRestore)
        .unwrap();
    let marker = std::fs::read_to_string(dir.join(pos_core::db::RESTORE_MARKER)).unwrap();
    drop(st);
    Restored {
        paths,
        owner,
        source: made.path,
        replaced,
        pre_restore_file: saved.file_name,
        marker,
    }
}

/// Review findings: the restored database journals the restore itself, naming the requester
/// and anchoring the journal it replaced (kept in the pre-restore copy).
#[test]
fn a_restore_is_journaled_with_its_requester_and_the_replaced_journal() {
    let dir = tempfile::tempdir().unwrap();
    let done = restore_and_stop(dir.path());
    let st = AppState::open(&done.paths, crate::printing::silent()).unwrap();
    let restored = audit_rows(&st, "backup.restored");
    assert_eq!(restored.len(), 1);
    let (user, payload) = &restored[0];
    assert_eq!(user.as_deref(), Some(done.owner.user_id.as_str()));
    assert_eq!(
        payload,
        &json!({
            "restoreId": done.marker.lines().nth(1).unwrap(),
            "requestedBy": done.owner.user_id,
            "source": done.source,
            "preRestoreFile": done.pre_restore_file,
            "replacedLastSeq": done.replaced.0,
            "replacedLastHash": done.replaced.1,
            "replacedMarks": { "counters": [], "chains": [], "closures": [] },
            "reissued": [],
        })
    );
    assert!(
        audit_rows(&st, "user.language_changed").is_empty(),
        "the replaced tail is not in the restored journal"
    );
    let report = st.db.read(pos_core::audit::verify_chain).unwrap();
    assert_eq!(report.first_broken_seq, None);
    // The anchor names the last row of the copy that keeps the replaced journal.
    let copy = std::path::Path::new(&done.paths.backups_dir).join(&done.pre_restore_file);
    let copy = pos_core::db::Db::open(
        &copy,
        pos_core::db::DbOptions {
            readers: 1,
            backup_dir: None,
        },
    )
    .unwrap();
    assert_eq!(tail_of(&copy), done.replaced);
}

/// The start that swapped the files crashed before its first write, and the marker survived:
/// the next start journals the restore once, and never again.
#[test]
fn a_restore_swapped_before_a_crash_is_journaled_once_at_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let done = restore_and_stop(dir.path());
    let marker_path = dir.path().join(pos_core::db::RESTORE_MARKER);
    let db_path = std::path::Path::new(&done.paths.db_path);
    assert!(
        pos_core::db::apply_pending_restore(db_path)
            .unwrap()
            .is_some()
    );
    std::fs::write(&marker_path, &done.marker).unwrap(); // its id is already applied
    let st = AppState::open(&done.paths, crate::printing::silent()).unwrap();
    let restored = audit_rows(&st, "backup.restored");
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].0.as_deref(), Some(done.owner.user_id.as_str()));
    assert_eq!(restored[0].1["replacedLastSeq"], done.replaced.0);
    assert!(!marker_path.exists(), "the applied marker is dropped");
    st.db.mark_clean_shutdown().unwrap();
    drop(st);
    let st = AppState::open(&done.paths, crate::printing::silent()).unwrap();
    assert_eq!(
        audit_rows(&st, "backup.restored").len(),
        1,
        "journaled exactly once"
    );
}

/// Review finding: a failed automatic backup reaches the banner, not only the log file.
#[test]
fn a_failed_automatic_backup_reaches_the_status() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    std::fs::write(
        &paths.backups_dir,
        b"a file where the backups folder should be",
    )
    .unwrap();
    let st = AppState::open(&paths, crate::printing::silent()).unwrap();
    first_run(&st);
    crate::state::run_daily_backup(&st);
    let status = backup::status(&st).unwrap();
    let last = status.last_auto.unwrap();
    assert_eq!((last.ok, last.error.as_deref()), (false, Some("io_error")));
    assert_eq!(status.newest_at, None);
    assert!(
        !status.database_damaged,
        "a folder problem is not a damaged database"
    );
    auth::logout(&st).unwrap();
    assert_eq!(backup::status(&st).unwrap_err().code(), "session_required");
}

#[test]
fn the_backup_status_turns_stale_after_two_business_days() {
    let dir = tempfile::tempdir().unwrap();
    let st = AppState::open(&AppPaths::new(dir.path()), crate::printing::silent()).unwrap();
    first_run(&st);
    crate::state::run_daily_backup(&st);
    let status = backup::status(&st).unwrap();
    let last = status.last_auto.unwrap();
    assert_eq!((last.ok, last.error), (true, None));
    let newest = status.newest_at.unwrap();
    assert!(!status.stale);
    let day = 86_400_000;
    assert!(!st.backup_status(newest + 2 * day).stale);
    assert!(st.backup_status(newest + 3 * day).stale);
}
