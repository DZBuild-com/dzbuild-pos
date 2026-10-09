//! First launch (spec §10.5 setup wizard): seed roles and the local terminal at every start,
//! then create company + settings + owner in one transaction.

use crate::audit::{self, Actor};
use crate::auth::users::{self, NewUser};
use crate::auth::{OWNER_ROLE_ID, Session, build_session, pin, roles};
use crate::company::{self, CompanyInput, PriceMode, ShopActivity, TaxRegime};
use crate::datapack::Pack;
use crate::db::{Db, meta_get, meta_set};
use crate::error::AppError;
use crate::lang::Lang;
use crate::settings;
use crate::terminal::{self, Terminal};
use crate::time::now_ms;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::fmt;
use std::sync::Arc;

pub const META_SETUP_COMPLETED_AT: &str = "setup_completed_at";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub completed: bool,
    pub has_owner: bool,
    pub terminal_code: Option<String>,
    pub login_lang: Lang,
}

/// The setup wizard's answers. `Debug` is written by hand so that the plaintext PIN never
/// reaches a log line.
#[derive(Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FirstRunInput {
    pub lang: Lang,
    pub shop_name: String,
    pub activity: ShopActivity,
    pub tax_regime: TaxRegime,
    pub price_mode: PriceMode,
    pub wilaya: Option<String>,
    pub phone: Option<String>,
    pub owner_name: String,
    pub owner_pin: String,
}

impl fmt::Debug for FirstRunInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FirstRunInput")
            .field("lang", &self.lang)
            .field("shop_name", &self.shop_name)
            .field("activity", &self.activity)
            .field("tax_regime", &self.tax_regime)
            .field("price_mode", &self.price_mode)
            .field("wilaya", &self.wilaya)
            .field("phone", &self.phone)
            .field("owner_name", &self.owner_name)
            .field("owner_pin", &"<redacted>")
            .finish()
    }
}

/// Idempotent start-up seeding: built-in roles and this PC's terminal. A till (M5) seeds
/// nothing: its roles and terminals are the main PC's, and it was adopted as its terminal.
pub fn bootstrap(db: &Db) -> Result<Terminal, AppError> {
    db.write(|tx| {
        if crate::sync::role::is_client(tx)? {
            return terminal::local(tx)?.ok_or_else(|| AppError::conflict("lan_not_paired"));
        }
        roles::seed_system_roles(tx)?;
        terminal::ensure_local(tx)
    })
}

fn has_active_owner(conn: &Connection) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM users WHERE role_id = ?1 AND active = 1)",
        [OWNER_ROLE_ID],
        |r| r.get(0),
    )?)
}

pub fn status(conn: &Connection) -> Result<SetupStatus, AppError> {
    Ok(SetupStatus {
        completed: meta_get(conn, META_SETUP_COMPLETED_AT)?.is_some(),
        has_owner: has_active_owner(conn)?,
        terminal_code: terminal::local(conn)?.map(|t| t.code),
        login_lang: settings::get(conn, "ui.login_lang")?,
    })
}

/// Creates the company, the wizard's settings and the first owner, and signs the owner in,
/// all in one transaction. Refused with `setup_already_done` once setup has completed or an
/// active owner exists, checked inside that same transaction.
pub fn complete_first_run(
    db: &Db,
    pack: Arc<Pack>,
    terminal: &Terminal,
    input: FirstRunInput,
) -> Result<Session, AppError> {
    let pin_hash = pin::hash_pin(&input.owner_pin)?;
    let terminal = terminal.clone();
    db.write(move |tx| {
        if meta_get(tx, META_SETUP_COMPLETED_AT)?.is_some() || has_active_owner(tx)? {
            return Err(AppError::conflict("setup_already_done"));
        }
        let system = Actor::system(Some(terminal.id.clone()));
        let mut company = CompanyInput::named(input.lang, input.shop_name.trim());
        company.tax_regime = input.tax_regime;
        company.price_mode = input.price_mode;
        company.wilaya = input.wilaya.clone();
        company.phone = input.phone.clone();
        company::save(tx, &system, &pack, company)?;
        settings::set_value(tx, &system, "ui.login_lang", json!(input.lang))?;
        crate::packs::apply_template(tx, &system, &pack, input.activity, input.lang, true)?;
        let owner = NewUser {
            username: None,
            display_name: input.owner_name.clone(),
            pin: String::new(), // already hashed above, outside the writer
            role_id: OWNER_ROLE_ID.to_owned(),
            lang: input.lang,
            discount_limit_bp: 10_000,
        };
        let owner = users::create(tx, &system, &owner, &pin_hash)?;
        let now = now_ms();
        meta_set(tx, META_SETUP_COMPLETED_AT, &now.to_string())?;
        tx.execute(
            "UPDATE users SET last_login_at = ?2 WHERE id = ?1",
            params![owner.id, now],
        )?;
        let session = build_session(tx, &owner.id, &terminal, now)?;
        audit::append(
            tx,
            &session.actor().event(
                "setup.completed",
                "company",
                Some("1"),
                json!({
                    "activity": input.activity,
                    "taxRegime": input.tax_regime,
                    "priceMode": input.price_mode,
                    "lang": input.lang
                }),
            ),
        )?;
        Ok(session)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> FirstRunInput {
        FirstRunInput {
            lang: Lang::Ar,
            shop_name: "  مغازة النور ".into(),
            activity: ShopActivity::Butcher,
            tax_regime: TaxRegime::Ifu,
            price_mode: PriceMode::Ttc,
            wilaya: Some("16".into()),
            phone: Some("0555 12 34 56".into()),
            owner_name: "Amine Benali".into(),
            owner_pin: "2580".into(),
        }
    }

    fn pack() -> Arc<Pack> {
        Arc::new(crate::datapack::bundled().unwrap())
    }

    fn count(db: &Db, sql: &'static str) -> i64 {
        db.read(move |c| Ok(c.query_row(sql, [], |r| r.get(0))?))
            .unwrap()
    }

    #[test]
    fn bootstrap_is_idempotent() {
        let db = Db::open_memory().unwrap();
        let first = bootstrap(&db).unwrap();
        let second = bootstrap(&db).unwrap();
        assert_eq!(first, second);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM roles"), 8);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM terminals"), 1);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM audit_log"), 0);
    }

    #[test]
    fn first_run_creates_company_owner_and_settings_at_once() {
        let db = Db::open_memory().unwrap();
        let terminal = bootstrap(&db).unwrap();
        let before = db.read(status).unwrap();
        assert!(!before.completed && !before.has_owner);
        assert_eq!(
            (before.terminal_code.as_deref(), before.login_lang),
            (Some("C1"), Lang::Fr)
        );

        let session = complete_first_run(&db, pack(), &terminal, input()).unwrap();
        assert!(session.is_owner);
        assert_eq!(
            (session.lang, session.display_name.as_str()),
            (Lang::Ar, "Amine Benali")
        );

        let after = db.read(status).unwrap();
        assert!(after.completed && after.has_owner);
        assert_eq!(after.login_lang, Lang::Ar);
        let company = db.read(company::get).unwrap().unwrap();
        assert_eq!(
            (company.name_ar.as_str(), company.name_fr.as_str()),
            ("مغازة النور", "")
        );
        assert_eq!(
            (company.tax_regime, company.default_doc_lang),
            (TaxRegime::Ifu, Lang::Ar)
        );
        assert_eq!(
            db.read(|c| settings::get::<ShopActivity>(c, "shop.activity"))
                .unwrap(),
            ShopActivity::Butcher
        );
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM categories WHERE name_fr = 'Volaille'"
            ),
            1
        );
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM audit_log WHERE action = 'packs.template_applied'"
            ),
            1
        );
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM audit_log WHERE action = 'setup.completed'"
            ),
            1
        );
        let again = crate::auth::login(&db, &terminal, &session.user_id, "2580").unwrap();
        assert!(again.is_owner);
    }

    #[test]
    fn second_first_run_is_refused() {
        let db = Db::open_memory().unwrap();
        let terminal = bootstrap(&db).unwrap();
        complete_first_run(&db, pack(), &terminal, input()).unwrap();
        let err = complete_first_run(&db, pack(), &terminal, input()).unwrap_err();
        assert_eq!(err.code(), "setup_already_done");
        assert_eq!(count(&db, "SELECT COUNT(*) FROM users"), 1);
    }

    #[test]
    fn an_existing_owner_blocks_first_run() {
        let db = Db::open_memory().unwrap();
        let terminal = bootstrap(&db).unwrap();
        let hash = pin::hash_pin("2580").unwrap();
        db.write(move |tx| {
            let owner = NewUser {
                username: None,
                display_name: "Nadia".into(),
                pin: String::new(),
                role_id: OWNER_ROLE_ID.to_owned(),
                lang: Lang::Fr,
                discount_limit_bp: 10_000,
            };
            users::create(tx, &Actor::default(), &owner, &hash)
        })
        .unwrap();
        assert!(!db.read(status).unwrap().completed);
        let err = complete_first_run(&db, pack(), &terminal, input()).unwrap_err();
        assert_eq!(err.code(), "setup_already_done");
        assert_eq!(count(&db, "SELECT COUNT(*) FROM users"), 1);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM company"), 0);
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM audit_log WHERE action <> 'user.created'"
            ),
            0
        );
    }

    #[test]
    fn failed_first_run_leaves_nothing_behind() {
        let db = Db::open_memory().unwrap();
        let terminal = bootstrap(&db).unwrap();
        let mut bad = input();
        bad.wilaya = Some("99".into());
        assert_eq!(
            complete_first_run(&db, pack(), &terminal, bad)
                .unwrap_err()
                .code(),
            "wilaya_invalid"
        );
        let mut weak = input();
        weak.owner_pin = "1234".into();
        assert_eq!(
            complete_first_run(&db, pack(), &terminal, weak)
                .unwrap_err()
                .code(),
            "pin_too_simple"
        );
        assert_eq!(count(&db, "SELECT COUNT(*) FROM users"), 0);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM company"), 0);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM audit_log"), 0);
        assert!(!db.read(status).unwrap().completed);
    }

    #[test]
    fn first_run_input_debug_hides_the_pin() {
        let printed = format!("{:?}", input());
        assert!(
            !printed.contains("2580") && printed.contains("<redacted>"),
            "{printed}"
        );
        assert!(printed.contains("Amine Benali"), "{printed}");
    }
}
