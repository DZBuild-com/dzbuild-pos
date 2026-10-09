//! The demo shop (ONB-002): a separate database under `<data>\demo\` that the app opens instead
//! of the real one while `demo.active` exists, seeded through the same services as a real shop
//! (numbering, hash chains, Z closures), named "DÉMO · …" on every screen and receipt, and
//! erased as a whole folder once the trainee leaves. The real `pos.db` is never touched.

use crate::auth::users::{self, NewUser};
use crate::auth::{CASHIER_ROLE_ID, OWNER_ROLE_ID, build_session, hash_pin};
use crate::cash::{self, CloseInput, CountLine};
use crate::catalog::categories::{self, CategoryInput};
use crate::catalog::codes::ean13_check_digit;
use crate::catalog::products::{self, Barcode, ProductInput};
use crate::catalog::{UNIT_PIECE_ID, tax_rates};
use crate::company::{CompanyInput, PriceMode, ShopActivity, TaxRegime};
use crate::customers::{self, CustomerInput};
use crate::datapack::Pack;
use crate::db::Db;
use crate::error::AppError;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::PaymentInput;
use crate::sales::{Cart, CartLine, SaleInput, create_sale};
use crate::setup::{self, FirstRunInput};
use crate::stock::{self, AdjustInput, AdjustReason};
use crate::terminal::Terminal;
use crate::till::TillContext;
use crate::time::{ALGIERS_OFFSET_MS, business_date};
use rusqlite::Connection;
use serde::Serialize;
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const DIR: &str = "demo";
pub const MARKER: &str = "demo.active";
pub const OWNER_PIN: &str = "2580";
pub const CASHIER_PIN: &str = "4826";
/// Every name of the demo shop starts with it, and keeps it (finding 39).
pub const NAME_PREFIX: &str = "DÉMO · ";
pub const SHOP_NAME: &str = "DÉMO · Supérette El Baraka";
/// `app_meta` key marking the demo shop's own database (`1`), written by [`seed`] and at every
/// start of the demo profile. A marked database is a training shop with no fiscal value: its
/// names keep [`NAME_PREFIX`], it takes no NIF/RC/NIS/AI/RIB, numbers no invoice or credit note,
/// every print of it says "DÉMO – sans valeur fiscale", and no restore puts it in the real
/// shop's place (nor the real shop's in the demo's).
pub const META_DEMO: &str = "demo";

/// The refusal of anything the demo shop must not do (one code, translated in ar/fr/en).
pub fn not_allowed() -> AppError {
    AppError::conflict("demo_not_allowed")
}

/// This database is the demo shop's ([`META_DEMO`]).
pub fn is_marked(conn: &Connection) -> Result<bool, AppError> {
    Ok(crate::db::meta_get(conn, META_DEMO)?.as_deref() == Some("1"))
}

/// Marks this database as the demo shop's (idempotent).
pub fn mark(conn: &Connection) -> Result<(), AppError> {
    crate::db::meta_set(conn, META_DEMO, "1")
}

/// `demo_not_allowed` in a marked database.
pub fn refuse_in_demo(conn: &Connection) -> Result<(), AppError> {
    if is_marked(conn)? {
        Err(not_allowed())
    } else {
        Ok(())
    }
}

/// The demo shop's identity rule (finding 39): every non-empty name keeps [`NAME_PREFIX`], and
/// no fiscal identifier (NIF, RC, NIS, AI, RIB) is ever recorded, so nothing printed from it can
/// pass for a real shop's document.
pub fn check_company(input: &CompanyInput) -> Result<(), AppError> {
    let names = [&input.name_ar, &input.name_fr, &input.name_en];
    let renamed = names
        .iter()
        .any(|name| !name.trim().is_empty() && !name.trim().starts_with(NAME_PREFIX.trim_end()));
    let fiscal = [&input.rc, &input.nif, &input.nis, &input.ai, &input.rib]
        .iter()
        .any(|value| value.as_deref().is_some_and(|v| !v.trim().is_empty()));
    if renamed || fiscal {
        return Err(not_allowed());
    }
    Ok(())
}

pub fn data_dir_of(data_dir: &Path) -> PathBuf {
    data_dir.join(DIR)
}

pub fn is_active(data_dir: &Path) -> bool {
    data_dir.join(MARKER).exists()
}

pub fn exists(data_dir: &Path) -> bool {
    data_dir_of(data_dir).exists()
}

/// The next start opens the demo shop.
pub fn activate(data_dir: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(data_dir_of(data_dir))?;
    std::fs::write(data_dir.join(MARKER), b"demo")?;
    Ok(())
}

/// Left by `demo_leave` in the data folder (the demo's own database is the one open then): the
/// real shop's next start journals `demo.left` from it in the real `pos.db`, then removes it.
pub const LEFT_NOTE: &str = "demo.left";

/// Notes that the demo was left at `at` by `user_name` (journaled by the real shop's next start).
pub fn note_left(data_dir: &Path, at: i64, user_name: Option<&str>) -> Result<(), AppError> {
    let note = serde_json::json!({ "at": at, "userName": user_name });
    std::fs::write(data_dir.join(LEFT_NOTE), serde_json::to_vec(&note)?)?;
    Ok(())
}

/// The note [`note_left`] wrote; `None` when there is none.
pub fn left_note(data_dir: &Path) -> Option<serde_json::Value> {
    let bytes = std::fs::read(data_dir.join(LEFT_NOTE)).ok()?;
    Some(serde_json::from_slice(&bytes).unwrap_or_else(|_| serde_json::json!({})))
}

/// Removes the note once its row is journaled.
pub fn forget_left_note(data_dir: &Path) {
    if let Err(error) = std::fs::remove_file(data_dir.join(LEFT_NOTE))
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "could not remove the demo's leave note");
    }
}

/// The next start opens the real shop again; the demo stays until erased.
pub fn deactivate(data_dir: &Path) -> Result<(), AppError> {
    match std::fs::remove_file(data_dir.join(MARKER)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

/// Deletes the demo folder (database, backups). Refused while the demo is the shop in use.
pub fn erase(data_dir: &Path) -> Result<bool, AppError> {
    if is_active(data_dir) {
        return Err(AppError::conflict("demo_active"));
    }
    let folder = data_dir_of(data_dir);
    if !folder.exists() {
        return Ok(false);
    }
    std::fs::remove_dir_all(folder)?;
    Ok(true)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DemoSummary {
    pub owner_id: String,
    pub owner_name: String,
    pub cashier_name: String,
    pub products: i64,
    pub customers: i64,
    pub tickets: i64,
    pub closures: i64,
}

const CATEGORIES: [(&str, &str, &str); 5] = [
    ("Épicerie", "بقالة", "Grocery"),
    ("Boissons", "مشروبات", "Drinks"),
    ("Produits laitiers", "مشتقات الحليب", "Dairy"),
    ("Hygiène", "نظافة", "Hygiene"),
    ("Confiserie", "حلويات", "Sweets"),
];

/// (French, Arabic, English, category, price TTC in centimes, tax rate code).
const PRODUCTS: [(&str, &str, &str, usize, i64, &str); 30] = [
    (
        "Semoule moyenne 1 kg",
        "سميد متوسط 1 كغ",
        "Medium semolina 1 kg",
        0,
        12_000,
        "TVA9",
    ),
    ("Farine 1 kg", "فرينة 1 كغ", "Flour 1 kg", 0, 9_000, "TVA9"),
    (
        "Sucre blanc 1 kg",
        "سكر أبيض 1 كغ",
        "White sugar 1 kg",
        0,
        11_000,
        "TVA9",
    ),
    (
        "Huile de table 2 L",
        "زيت المائدة 2 ل",
        "Cooking oil 2 L",
        0,
        36_000,
        "TVA9",
    ),
    (
        "Pâtes spaghetti 500 g",
        "معكرونة سباغيتي 500 غ",
        "Spaghetti 500 g",
        0,
        7_500,
        "TVA19",
    ),
    ("Riz 1 kg", "أرز 1 كغ", "Rice 1 kg", 0, 22_000, "TVA19"),
    (
        "Lentilles 1 kg",
        "عدس 1 كغ",
        "Lentils 1 kg",
        0,
        30_000,
        "TVA19",
    ),
    (
        "Café moulu 250 g",
        "قهوة مطحونة 250 غ",
        "Ground coffee 250 g",
        0,
        45_000,
        "TVA19",
    ),
    (
        "Thé vert 250 g",
        "شاي أخضر 250 غ",
        "Green tea 250 g",
        0,
        32_000,
        "TVA19",
    ),
    (
        "Concentré de tomate 400 g",
        "طماطم مصبرة 400 غ",
        "Tomato paste 400 g",
        0,
        16_000,
        "TVA19",
    ),
    (
        "Eau minérale 1,5 L",
        "مياه معدنية 1.5 ل",
        "Mineral water 1.5 L",
        1,
        4_000,
        "TVA19",
    ),
    (
        "Boisson gazeuse 1 L",
        "مشروب غازي 1 ل",
        "Soft drink 1 L",
        1,
        12_000,
        "TVA19",
    ),
    (
        "Jus d'orange 1 L",
        "عصير برتقال 1 ل",
        "Orange juice 1 L",
        1,
        18_000,
        "TVA19",
    ),
    (
        "Eau gazeuse 1 L",
        "مياه غازية 1 ل",
        "Sparkling water 1 L",
        1,
        7_000,
        "TVA19",
    ),
    (
        "Boisson énergétique 25 cl",
        "مشروب طاقة 25 سل",
        "Energy drink 25 cl",
        1,
        15_000,
        "TVA19",
    ),
    (
        "Lait en sachet 1 L",
        "حليب في كيس 1 ل",
        "Milk in a bag 1 L",
        2,
        2_500,
        "TVA9",
    ),
    ("Lben 1 L", "لبن 1 ل", "Lben 1 L", 2, 11_000, "TVA9"),
    (
        "Yaourt nature",
        "ياغورت طبيعي",
        "Plain yoghurt",
        2,
        3_000,
        "TVA19",
    ),
    (
        "Fromage en portions x8",
        "جبن مثلثات ×8",
        "Cheese portions x8",
        2,
        21_000,
        "TVA19",
    ),
    (
        "Beurre 250 g",
        "زبدة 250 غ",
        "Butter 250 g",
        2,
        38_000,
        "TVA19",
    ),
    (
        "Savon 125 g",
        "صابون 125 غ",
        "Soap 125 g",
        3,
        9_000,
        "TVA19",
    ),
    (
        "Shampooing 400 ml",
        "شامبو 400 مل",
        "Shampoo 400 ml",
        3,
        35_000,
        "TVA19",
    ),
    (
        "Lessive en poudre 1 kg",
        "مسحوق الغسيل 1 كغ",
        "Washing powder 1 kg",
        3,
        42_000,
        "TVA19",
    ),
    (
        "Eau de Javel 1 L",
        "ماء جافيل 1 ل",
        "Bleach 1 L",
        3,
        10_000,
        "TVA19",
    ),
    (
        "Papier toilette x4",
        "ورق صحي ×4",
        "Toilet paper x4",
        3,
        25_000,
        "TVA19",
    ),
    (
        "Biscuits au chocolat",
        "بسكويت بالشوكولاتة",
        "Chocolate biscuits",
        4,
        6_000,
        "TVA19",
    ),
    (
        "Chocolat en tablette 100 g",
        "لوح شوكولاتة 100 غ",
        "Chocolate bar 100 g",
        4,
        20_000,
        "TVA19",
    ),
    ("Chewing-gum", "علكة", "Chewing gum", 4, 2_000, "TVA19"),
    (
        "Bonbons 200 g",
        "حلوى 200 غ",
        "Sweets 200 g",
        4,
        14_000,
        "TVA19",
    ),
    ("Gaufrettes", "بسكويت ويفر", "Wafers", 4, 5_000, "TVA19"),
];

/// (French, Arabic, English name, phone): fictitious numbers of the demo only.
const CUSTOMERS: [(&str, &str, &str, Option<&str>); 3] = [
    (
        "Karim B. (démo)",
        "كريم ب. (تجريبي)",
        "Karim B. (demo)",
        Some("0555000001"),
    ),
    (
        "Épicerie du coin (démo)",
        "بقالة الحي (تجريبي)",
        "Corner grocery (demo)",
        None,
    ),
    (
        "Samia H. (démo)",
        "سامية ح. (تجريبي)",
        "Samia H. (demo)",
        Some("0555000003"),
    ),
];

/// The demo's owner and cashier in the trainee's language: the bracket says "demo" and the
/// initials on the login tiles skip it.
fn people(lang: Lang) -> (&'static str, &'static str) {
    match lang {
        Lang::Ar => ("أمين (تجريبي)", "ياسين (تجريبي)"),
        Lang::Fr => ("Amine (démo)", "Yacine (démo)"),
        Lang::En => ("Amine (demo)", "Yacine (demo)"),
    }
}

fn in_lang(lang: Lang, fr: &'static str, ar: &'static str, en: &'static str) -> &'static str {
    match lang {
        Lang::Ar => ar,
        Lang::Fr => fr,
        Lang::En => en,
    }
}

const DAYS: i64 = 6;
const TICKETS_PER_DAY: i64 = 8;
const DAY_MS: i64 = 86_400_000;

/// A fixed pseudo-random sequence: every demo is the same, so a trainer can say "ticket 12".
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % n.max(1)
    }
}

/// `hour:minute` Algiers time on the business day `days_ago` days before `now`, as ms.
fn at(now: i64, days_ago: i64, hour: i64, minute: i64) -> i64 {
    let midnight = business_date(now)
        .checked_sub_days(chrono::Days::new(days_ago.unsigned_abs()))
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .map(|d| d.and_utc().timestamp_millis() - ALGIERS_OFFSET_MS)
        .unwrap_or(now - days_ago * DAY_MS);
    midnight + hour * 3_600_000 + minute * 60_000
}

fn counts(db: &Db, owner_id: String) -> Result<DemoSummary, AppError> {
    db.read(move |c| {
        let one = |sql: &str| -> Result<i64, AppError> { Ok(c.query_row(sql, [], |r| r.get(0))?) };
        let owner_name: String = c.query_row(
            "SELECT display_name FROM users WHERE id = ?1",
            [&owner_id],
            |r| r.get(0),
        )?;
        let cashier_name: String = c.query_row(
            "SELECT display_name FROM users WHERE role_id = ?1 ORDER BY created_at LIMIT 1",
            [CASHIER_ROLE_ID],
            |r| r.get(0),
        )?;
        Ok(DemoSummary {
            owner_id,
            owner_name,
            cashier_name,
            products: one("SELECT COUNT(*) FROM products")?,
            customers: one("SELECT COUNT(*) FROM customers")?,
            tickets: one(
                "SELECT COUNT(*) FROM documents WHERE doc_type = 'ticket' AND status = 'validated'",
            )?,
            closures: one("SELECT COUNT(*) FROM cash_closures")?,
        })
    })
}

/// Seeds the demo shop into a fresh database (see the module doc); on a database whose setup is
/// done, only reads the summary back.
pub fn seed(
    db: &Db,
    pack: Arc<Pack>,
    terminal: &Terminal,
    lang: Lang,
    now: i64,
) -> Result<DemoSummary, AppError> {
    // First: the company saved by the first run below is checked against the demo's rule.
    db.write(|tx| mark(tx))?;
    if db.read(setup::status)?.completed {
        let owner: String = db.read(|c| {
            Ok(c.query_row(
                "SELECT id FROM users WHERE role_id = ?1 ORDER BY created_at LIMIT 1",
                [OWNER_ROLE_ID],
                |r| r.get(0),
            )?)
        })?;
        return counts(db, owner);
    }
    let owner = setup::complete_first_run(
        db,
        pack.clone(),
        terminal,
        FirstRunInput {
            lang,
            shop_name: SHOP_NAME.into(),
            activity: ShopActivity::General,
            tax_regime: TaxRegime::Ifu,
            price_mode: PriceMode::Ttc,
            wilaya: Some("16".into()),
            phone: None,
            owner_name: people(lang).0.into(),
            owner_pin: OWNER_PIN.into(),
        },
    )?;
    let actor = owner.actor();
    let cashier_hash = hash_pin(CASHIER_PIN)?;
    // The opening stock is counted before the first demo day, so the ledger never runs negative.
    let stocked_at = at(now, DAYS, 8, 0);
    let seed_pack = pack.clone();
    let (cashier_id, product_ids) = db.write(move |tx| {
        // The pack's VAT rates, as every start seeds them (idempotent).
        crate::catalog::ensure_defaults(tx, &seed_pack)?;
        let cashier = users::create(
            tx,
            &actor,
            &NewUser {
                username: None,
                display_name: people(lang).1.into(),
                pin: String::new(), // hashed above, outside the writer
                role_id: CASHIER_ROLE_ID.into(),
                lang,
                discount_limit_bp: 1_000,
            },
            &cashier_hash,
        )?;
        let mut category_ids = Vec::new();
        for (i, (fr, ar, en)) in CATEGORIES.iter().enumerate() {
            let category = categories::create(
                tx,
                &actor,
                CategoryInput {
                    parent_id: None,
                    name_ar: (*ar).into(),
                    name_fr: (*fr).into(),
                    name_en: (*en).into(),
                    color: None,
                    sort_order: i64::try_from(i).unwrap_or(0),
                    image: None,
                },
            )?;
            category_ids.push(category.id);
        }
        let mut product_ids = Vec::new();
        for (i, (fr, ar, en, category, price, rate)) in PRODUCTS.iter().enumerate() {
            let rate_id = tax_rates::by_code(tx, rate)?
                .map(|r| r.id)
                .ok_or_else(|| AppError::not_found("tax_rate"))?;
            let first12 = format!("2000000{:05}", i + 1);
            let check = ean13_check_digit(&first12)
                .ok_or_else(|| AppError::Internal("demo barcode".into()))?;
            let cost = Money(price * 70 / 100);
            let mut input = ProductInput::simple(fr, UNIT_PIECE_ID, &rate_id, Money(*price));
            input.name_ar = (*ar).into();
            input.name_en = (*en).into();
            input.category_id = category_ids.get(*category).cloned();
            input.cost_price = cost;
            input.barcodes = vec![Barcode {
                barcode: format!("{first12}{check}"),
                pack_qty: Qty(1_000),
                label: None,
            }];
            let product = products::create(tx, &actor, input)?;
            stock::adjust(
                tx,
                &actor,
                AdjustInput {
                    product_id: product.id.clone(),
                    location_id: None,
                    qty_delta: Qty(50_000),
                    reason: AdjustReason::Opening,
                    unit_cost: Some(cost),
                    note: None,
                },
                stocked_at,
            )?;
            product_ids.push(product.id);
        }
        for (fr, ar, en, phone) in CUSTOMERS {
            let mut input = CustomerInput::named(in_lang(lang, fr, ar, en));
            input.phone = phone.map(str::to_owned);
            customers::create(tx, &actor, &seed_pack, input)?;
        }
        Ok((cashier.id, product_ids))
    })?;
    let mut lcg = Lcg(0x2026_0928);
    for days_ago in (1..=DAYS).rev() {
        let opened = at(now, days_ago, 8, 30);
        let session = db.read({
            let (cashier_id, terminal) = (cashier_id.clone(), terminal.clone());
            move |c| build_session(c, &cashier_id, &terminal, opened)
        })?;
        let ctx = TillContext::new(session.clone(), pack.clone(), opened);
        db.write(move |tx| cash::open_session(tx, &ctx, Money(500_000)).map(drop))?;
        for ticket in 0..TICKETS_PER_DAY {
            let when = at(now, days_ago, 9, ticket * 40);
            let mut lines = Vec::new();
            for _ in 0..=lcg.below(3) {
                let index = usize::try_from(lcg.below(product_ids.len() as u64)).unwrap_or(0);
                let qty = i64::try_from(lcg.below(3) + 1).unwrap_or(1) * 1_000;
                if let Some(product_id) = product_ids.get(index) {
                    lines.push(CartLine::product(product_id, Qty(qty)));
                }
            }
            let ctx = TillContext::new(session.clone(), pack.clone(), when);
            let input = SaleInput {
                cart: Cart::new(lang, lines),
                payments: vec![PaymentInput::cash(Money(1_000_000))],
            };
            db.write(move |tx| create_sale(tx, &ctx, input).map(drop))?;
        }
        let closing = at(now, days_ago, 20, 0);
        let ctx = TillContext::new(session, pack.clone(), closing);
        db.write(move |tx| {
            let open = cash::require_open(tx, &ctx.session.terminal_id)?;
            let expected = cash::expected_cash(tx, &open.id)?;
            cash::close_session(
                tx,
                &ctx,
                CloseInput {
                    counts: vec![CountLine {
                        denomination: expected,
                        count: 1,
                    }],
                    notes: Some(SHOP_NAME.into()),
                },
            )
            .map(drop)
        })?;
    }
    counts(db, owner.user_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::documents::{self, DocType};

    /// 2026-09-28 12:00 Algiers.
    const NOW: i64 = 1_790_593_200_000;

    #[test]
    fn the_demo_shop_is_seeded_once_and_every_chain_verifies() {
        let db = Db::open_memory().unwrap();
        let terminal = crate::setup::bootstrap(&db).unwrap();
        let pack = Arc::new(crate::datapack::bundled().unwrap());
        let summary = seed(&db, pack.clone(), &terminal, Lang::Fr, NOW).unwrap();
        // M8: the first run applies the supérette template, whose seeds (regulated staples,
        // department keys, top-ups, services) come on top of the demo's own 30 products.
        let seeded = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM products WHERE reference LIKE 'SVC-%'
                        OR id IN (SELECT product_id FROM open_price_products)
                        OR id IN (SELECT product_id FROM product_regulations)",
                    [],
                    |r| r.get::<_, i64>(0),
                )?)
            })
            .unwrap();
        assert_eq!(
            (
                summary.products - seeded,
                summary.customers,
                summary.tickets,
                summary.closures
            ),
            (30, 3, 48, 6)
        );
        let again = seed(&db, pack, &terminal, Lang::Fr, NOW + 60_000).unwrap();
        assert_eq!(again.tickets, 48, "a second seed changes nothing");
        let company: String = db
            .read(
                |c| Ok(c.query_row("SELECT name_fr FROM company WHERE id = 1", [], |r| r.get(0))?),
            )
            .unwrap();
        assert!(company.starts_with("DÉMO"), "{company}");
        let tickets = db
            .read(|c| documents::verify_chain(c, DocType::Ticket, "C1"))
            .unwrap();
        assert_eq!(tickets.first_broken_seq, None);
        let journal = db.read(crate::audit::verify_chain).unwrap();
        assert_eq!(journal.first_broken_seq, None);
        let open: i64 = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM cash_sessions WHERE status = 'open'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(open, 0, "today's drawer is for the trainee to open");
        let owner = crate::auth::login(&db, &terminal, &summary.owner_id, OWNER_PIN).unwrap();
        assert!(owner.is_owner);
        let arabic: i64 = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM products WHERE name_ar <> '' AND name_en <> ''",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(
            arabic, summary.products,
            "every product is named in the three languages"
        );
    }

    /// Finding 39: the demo database is marked, its names keep "DÉMO · ", and no fiscal
    /// identifier is ever recorded in it; a real shop's database is not marked and keeps its
    /// rules.
    #[test]
    fn the_demo_shop_keeps_its_prefix_and_takes_no_fiscal_identity() {
        let db = Db::open_memory().unwrap();
        let terminal = crate::setup::bootstrap(&db).unwrap();
        let pack = Arc::new(crate::datapack::bundled().unwrap());
        seed(&db, pack.clone(), &terminal, Lang::Ar, NOW).unwrap();
        assert!(db.read(is_marked).unwrap());
        let company = db.read(crate::company::get).unwrap().unwrap();
        assert!(
            company.demo,
            "the print models read the mark from the company"
        );
        assert!(company.name_ar.starts_with(NAME_PREFIX), "{company:?}");
        let save = |change: &dyn Fn(&mut CompanyInput)| {
            let mut input = CompanyInput::from(company.clone());
            change(&mut input);
            let pack = pack.clone();
            db.write(move |tx| {
                crate::company::save(tx, &crate::audit::Actor::system(None), &pack, input)
            })
        };
        let refused = |change: &dyn Fn(&mut CompanyInput)| save(change).unwrap_err().code();
        assert_eq!(
            refused(&|c| c.name_ar = "محل البركة".into()),
            "demo_not_allowed"
        );
        assert_eq!(
            refused(&|c| c.name_fr = "Supérette El Baraka".into()),
            "demo_not_allowed",
            "a second language without the prefix"
        );
        assert_eq!(
            refused(&|c| c.nif = Some("000116001234567".into())),
            "demo_not_allowed"
        );
        assert_eq!(
            refused(&|c| c.rc = Some("16/00-1".into())),
            "demo_not_allowed"
        );
        assert_eq!(
            refused(&|c| c.nis = Some("000116010012345".into())),
            "demo_not_allowed"
        );
        assert_eq!(
            refused(&|c| c.ai = Some("16012345678".into())),
            "demo_not_allowed"
        );
        assert_eq!(
            refused(&|c| c.rib = Some("00799999".into())),
            "demo_not_allowed"
        );
        let renamed = save(&|c| {
            c.name_ar = format!("{NAME_PREFIX}محل البركة");
            c.phone = Some("0555000000".into());
        })
        .unwrap();
        assert!(renamed.name_ar.ends_with("محل البركة") && renamed.demo);
        let real = Db::open_memory().unwrap();
        crate::setup::bootstrap(&real).unwrap();
        assert!(!real.read(is_marked).unwrap());
        assert_eq!(real.read(refuse_in_demo).map_err(|e| e.code()), Ok(()));
    }

    #[test]
    fn the_demo_folder_is_switched_and_erased_only_when_left() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        assert!(!is_active(data) && !exists(data));
        activate(data).unwrap();
        assert!(is_active(data) && data_dir_of(data).is_dir());
        std::fs::write(data_dir_of(data).join("pos.db"), b"demo data").unwrap();
        assert_eq!(erase(data).unwrap_err().code(), "demo_active");
        deactivate(data).unwrap();
        deactivate(data).unwrap(); // already left: fine
        assert!(erase(data).unwrap());
        assert!(!exists(data));
        assert!(!erase(data).unwrap(), "nothing left to erase");
        std::fs::write(data.join("pos.db"), b"the real shop").unwrap();
        activate(data).unwrap();
        deactivate(data).unwrap();
        erase(data).unwrap();
        assert_eq!(
            std::fs::read(data.join("pos.db")).unwrap(),
            b"the real shop",
            "never touched"
        );
    }
}
