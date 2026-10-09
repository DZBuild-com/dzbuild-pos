//! A ready shop for the service tests of M1: company (réel, TTC unless asked), the data pack's
//! tax rates, the local terminal C1, an owner and a cashier with their sessions. Test-only.

use crate::audit::Actor;
use crate::auth::session::{Session, build_session};
use crate::auth::users::{self, NewUser};
use crate::auth::{CASHIER_ROLE_ID, OWNER_ROLE_ID};
use crate::catalog::products::{self, Product, ProductInput};
use crate::catalog::{UNIT_PIECE_ID, tax_rates};
use crate::company::{self, CompanyInput, PriceMode, TaxRegime};
use crate::datapack::Pack;
use crate::db::Db;
use crate::lang::Lang;
use crate::money::Money;
use crate::terminal::{self, Terminal};
use serde_json::Value;
use std::sync::Arc;

/// A well-formed Argon2id PHC string (the user store refuses anything else); nobody logs in
/// with it in the service tests.
pub(crate) const FAKE_PIN_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$ZmFrZWhhc2hmYWtlaGFzaGZha2VoYXNoZmFrZWhhc2g";

/// 2026-09-28 12:00 Algiers (11:00 UTC): the default "now" of service tests.
pub(crate) const NOON: i64 = 1_790_593_200_000;

pub(crate) struct Shop {
    pub db: Db,
    pub pack: Arc<Pack>,
    pub terminal: Terminal,
    pub owner: Session,
    pub cashier: Session,
    pub tva19: String,
    pub tva9: String,
    pub exempt: String,
}

pub(crate) fn shop() -> Shop {
    shop_with(TaxRegime::Reel, PriceMode::Ttc)
}

pub(crate) fn shop_with(regime: TaxRegime, mode: PriceMode) -> Shop {
    shop_in(Db::open_memory().expect("memory db"), regime, mode)
}

/// The same shop in an existing database (a file for crash, backup and timing tests).
pub(crate) fn shop_in(db: Db, regime: TaxRegime, mode: PriceMode) -> Shop {
    let pack = Arc::new(crate::datapack::bundled().expect("bundled pack"));
    let seed_pack = Arc::clone(&pack);
    let (terminal, owner_id, cashier_id) = db
        .write(move |tx| {
            crate::auth::roles::seed_system_roles(tx)?;
            let terminal = terminal::ensure_local(tx)?;
            crate::catalog::ensure_defaults(tx, &seed_pack)?;
            let mut input = CompanyInput::named(Lang::Fr, "Supérette El Amel");
            input.name_ar = "مغازة الأمل".into();
            input.tax_regime = regime;
            input.price_mode = mode;
            input.nif = Some("000116001234567".into());
            input.address = Some("12 rue Larbi Ben M'hidi, Alger".into());
            company::save(tx, &Actor::system(None), &seed_pack, input)?;
            let person = |name: &str, role: &str, limit: i32| NewUser {
                username: None,
                display_name: name.into(),
                pin: String::new(),
                role_id: role.into(),
                lang: Lang::Fr,
                discount_limit_bp: limit,
            };
            let owner = users::create(
                tx,
                &Actor::system(None),
                &person("Amine", OWNER_ROLE_ID, 0),
                FAKE_PIN_HASH,
            )?;
            let cashier = users::create(
                tx,
                &Actor::system(None),
                &person("Yacine", CASHIER_ROLE_ID, 1_000),
                FAKE_PIN_HASH,
            )?;
            Ok((terminal, owner.id, cashier.id))
        })
        .expect("seed shop");
    let t = terminal.clone();
    let (owner, cashier) = db
        .read(move |c| {
            Ok((
                build_session(c, &owner_id, &t, NOON)?,
                build_session(c, &cashier_id, &t, NOON)?,
            ))
        })
        .expect("sessions");
    let rate = |code: &'static str| {
        db.read(move |c| Ok(tax_rates::by_code(c, code)?.map(|r| r.id)))
            .expect("rate")
            .expect("seeded rate")
    };
    let (tva19, tva9, exempt) = (rate("TVA19"), rate("TVA9"), rate("EXO"));
    Shop {
        db,
        pack,
        terminal,
        owner,
        cashier,
        tva19,
        tva9,
        exempt,
    }
}

impl Shop {
    /// A stocked piece at `price` (TTC in a TTC shop) with the given rate id.
    pub fn product(&self, name_fr: &str, price: Money, rate_id: &str) -> Product {
        self.product_with(ProductInput::simple(name_fr, UNIT_PIECE_ID, rate_id, price))
    }

    pub fn product_with(&self, input: ProductInput) -> Product {
        self.db
            .write(move |tx| products::create(tx, &Actor::system(None), input))
            .expect("product")
    }

    pub fn set(&self, key: &'static str, value: Value) {
        self.db
            .write(move |tx| crate::settings::set_value(tx, &Actor::system(None), key, value))
            .expect("setting");
    }

    /// Opening stock of `qty` milli-units at the default location.
    pub fn stock_in(&self, product_id: &str, qty: i64) {
        let input = crate::stock::AdjustInput {
            product_id: product_id.into(),
            location_id: None,
            qty_delta: crate::money::Qty(qty),
            reason: crate::stock::AdjustReason::Opening,
            unit_cost: None,
            note: None,
        };
        let actor = self.owner.actor();
        self.db
            .write(move |tx| crate::stock::adjust(tx, &actor, input, NOON))
            .expect("stock in");
    }

    /// The cashier at the till, at noon on 2026-09-28.
    pub fn till(&self) -> crate::till::TillContext {
        crate::till::TillContext::new(self.cashier.clone(), Arc::clone(&self.pack), NOON)
    }

    /// The owner's till context (every permission), at `NOON`.
    pub fn owner_till(&self) -> crate::till::TillContext {
        crate::till::TillContext::new(self.owner.clone(), Arc::clone(&self.pack), NOON)
    }

    /// The cashier's till with the owner's approval of `permission`.
    pub fn approved_till(&self, permission: crate::auth::Permission) -> crate::till::TillContext {
        self.till()
            .with_approval(crate::auth::session::OverrideGrant {
                approver_id: self.owner.user_id.clone(),
                approver_name: self.owner.display_name.clone(),
                permission,
                granted_at: NOON,
                discount_limit_bp: 10_000,
            })
    }

    /// A customer created by the owner, known by `name` only.
    pub fn customer(&self, name: &str) -> crate::customers::Customer {
        let input = crate::customers::CustomerInput::named(name);
        let pack = Arc::clone(&self.pack);
        let actor = self.owner.actor();
        self.db
            .write(move |tx| crate::customers::create(tx, &actor, &pack, input))
            .expect("customer")
    }

    /// The company with every mention an invoice needs (décret 05-468 art. 3): SARL with its
    /// capital, activity, address, phone, RC, NIF, NIS, AI and RIB.
    pub fn complete_company(&self) {
        let pack = Arc::clone(&self.pack);
        self.db
            .write(move |tx| {
                let mut input = CompanyInput::from(
                    company::get(tx)?
                        .ok_or_else(|| crate::error::AppError::conflict("setup_required"))?,
                );
                input.legal_form = Some("SARL".into());
                input.capital = Some(Money(100_000_000));
                input.phone = Some("023 45 67 89".into());
                input.rc = Some("16/00-1234567B26".into());
                input.nis = Some("000116001234567".into());
                input.ai = Some("16012345678".into());
                input.rib = Some("00799999001234567890".into());
                input.wilaya = Some("16".into());
                company::save(tx, &Actor::system(None), &pack, input).map(drop)
            })
            .expect("complete company");
        self.set(
            "company.activity",
            serde_json::json!({
                "ar": "تجارة التجزئة للمواد الغذائية العامة",
                "fr": "Commerce de détail en alimentation générale",
                "en": "General food retail"
            }),
        );
    }

    /// A reseller with NIF, RC, NIS, AI, legal form, activity and address: a B2B buyer.
    pub fn b2b_customer(&self, name: &str) -> crate::customers::Customer {
        let (pack, name) = (Arc::clone(&self.pack), name.to_owned());
        self.db
            .write(move |tx| {
                let mut input = crate::customers::CustomerInput::named(&name);
                input.customer_type = crate::customers::CustomerType::Reseller;
                input.address = Some("5 rue Didouche Mourad, Alger".into());
                input.wilaya = Some("16".into());
                input.nif = Some("000016001234567".into());
                input.rc = Some("16/00-7654321B26".into());
                input.nis = Some("000016007654321".into());
                input.ai = Some("16098765432".into());
                input.legal_form = Some("SARL".into());
                input.activity = Some("Quincaillerie".into());
                crate::customers::create(tx, &Actor::system(None), &pack, input)
            })
            .expect("b2b customer")
    }

    /// A private person with an address (the buyer of an invoice asked at the till).
    pub fn consumer(&self, name: &str) -> crate::customers::Customer {
        let (pack, name) = (Arc::clone(&self.pack), name.to_owned());
        self.db
            .write(move |tx| {
                let mut input = crate::customers::CustomerInput::named(&name);
                input.address = Some("Cité 200 logements, Bab Ezzouar".into());
                crate::customers::create(tx, &Actor::system(None), &pack, input)
            })
            .expect("consumer")
    }

    /// A supplier created by the owner, known by `name` only.
    pub fn supplier(&self, name: &str) -> crate::suppliers::Supplier {
        let input = crate::suppliers::SupplierInput::named(name);
        let pack = Arc::clone(&self.pack);
        let actor = self.owner.actor();
        self.db
            .write(move |tx| crate::suppliers::create(tx, &actor, &pack, input))
            .expect("supplier")
    }

    /// Opens the cashier's session with `float` centimes.
    pub fn open_till(&self, float: i64) -> crate::cash::CashSession {
        let ctx = self.till();
        self.db
            .write(move |tx| crate::cash::open_session(tx, &ctx, Money(float)))
            .expect("open till")
    }

    /// An open-price department key at TVA 19 % (the department keys module must be on); its
    /// product id.
    pub fn department_key(&self, name: &str) -> String {
        let input = crate::catalog::departments::DepartmentKeyInput {
            name_ar: name.into(),
            name_fr: name.into(),
            name_en: name.into(),
            category_id: None,
            tax_rate_id: self.tva19.clone(),
            unit_id: None,
            color: None,
            sort_order: 1,
        };
        let actor = self.owner.actor();
        self.db
            .write(move |tx| crate::catalog::departments::create(tx, &actor, input))
            .expect("department key")
            .product_id
    }

    pub fn count(&self, sql: &'static str) -> i64 {
        self.db
            .read(move |c| Ok(c.query_row(sql, [], |r| r.get(0))?))
            .expect("count")
    }
}

#[test]
fn the_test_shop_is_ready_to_sell() {
    let shop = shop();
    assert_eq!(shop.terminal.code, "C1");
    assert_eq!(shop.pack.id, "dz-2026.1");
    assert!(shop.owner.is_owner);
    assert!(!shop.cashier.is_owner);
    assert_eq!(shop.cashier.discount_limit_bp, 1_000);
    assert_eq!(shop.cashier.terminal_code, "C1");
    for (rate_id, bp) in [(&shop.tva19, 1_900), (&shop.tva9, 900), (&shop.exempt, 0)] {
        let p = shop.product("Article", Money(11_000), rate_id);
        assert_eq!(p.tax_rate_bp.0, bp);
    }
    shop.set("sales.cash_rounding", serde_json::json!(500));
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM settings WHERE key = 'sales.cash_rounding'"),
        1
    );
    assert_eq!(shop.count("SELECT COUNT(*) FROM users"), 2);
}
