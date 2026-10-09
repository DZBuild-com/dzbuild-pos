//! The shop's identity: the single `company` row (spec §5.1).

use crate::audit::{self, Actor};
use crate::datapack::Pack;
use crate::error::AppError;
use crate::lang::Lang;
use crate::money::Money;
use crate::sqlenum::sql_text_enum;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

pub const NAME_MAX: usize = 80;
pub const TEXT_MAX: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum TaxRegime {
    #[default]
    Reel,
    Ifu,
}

impl TaxRegime {
    pub const fn as_str(self) -> &'static str {
        match self {
            TaxRegime::Reel => "reel",
            TaxRegime::Ifu => "ifu",
        }
    }

    pub fn parse(s: &str) -> Option<TaxRegime> {
        [TaxRegime::Reel, TaxRegime::Ifu]
            .into_iter()
            .find(|v| v.as_str() == s)
    }
}

sql_text_enum!(TaxRegime);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum PriceMode {
    #[default]
    Ttc,
    Ht,
}

impl PriceMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            PriceMode::Ttc => "ttc",
            PriceMode::Ht => "ht",
        }
    }

    pub fn parse(s: &str) -> Option<PriceMode> {
        [PriceMode::Ttc, PriceMode::Ht]
            .into_iter()
            .find(|v| v.as_str() == s)
    }
}

sql_text_enum!(PriceMode);

/// Activity template chosen in the setup wizard (spec §10.5, master spec §30.2): one per pack.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum ShopActivity {
    /// Supérette, épicerie, alimentation générale.
    #[default]
    General,
    Butcher,
    Bakery,
    /// Restaurant, café, fast-food, pizzeria.
    FoodService,
    Parapharmacy,
    Pharmacy,
    /// Quincaillerie, matériaux de construction.
    Hardware,
    AutoParts,
    Clothing,
    Phones,
    Cosmetics,
    /// Librairie, papeterie.
    Stationery,
    Wholesale,
    Instalments,
    Optics,
    Pressing,
    Salon,
    Jewellery,
    Gym,
    /// Kiosque, tabac, multiservices.
    Kiosk,
    /// Consignes: butane gas, beverage crates.
    Containers,
    Importer,
    VanSales,
    /// Atelier de production.
    Workshop,
    Other,
}

impl ShopActivity {
    pub const ALL: [ShopActivity; 25] = [
        ShopActivity::General,
        ShopActivity::Butcher,
        ShopActivity::Bakery,
        ShopActivity::FoodService,
        ShopActivity::Parapharmacy,
        ShopActivity::Pharmacy,
        ShopActivity::Hardware,
        ShopActivity::AutoParts,
        ShopActivity::Clothing,
        ShopActivity::Phones,
        ShopActivity::Cosmetics,
        ShopActivity::Stationery,
        ShopActivity::Wholesale,
        ShopActivity::Instalments,
        ShopActivity::Optics,
        ShopActivity::Pressing,
        ShopActivity::Salon,
        ShopActivity::Jewellery,
        ShopActivity::Gym,
        ShopActivity::Kiosk,
        ShopActivity::Containers,
        ShopActivity::Importer,
        ShopActivity::VanSales,
        ShopActivity::Workshop,
        ShopActivity::Other,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ShopActivity::General => "general",
            ShopActivity::Butcher => "butcher",
            ShopActivity::Bakery => "bakery",
            ShopActivity::FoodService => "food_service",
            ShopActivity::Parapharmacy => "parapharmacy",
            ShopActivity::Pharmacy => "pharmacy",
            ShopActivity::Hardware => "hardware",
            ShopActivity::AutoParts => "auto_parts",
            ShopActivity::Clothing => "clothing",
            ShopActivity::Phones => "phones",
            ShopActivity::Cosmetics => "cosmetics",
            ShopActivity::Stationery => "stationery",
            ShopActivity::Wholesale => "wholesale",
            ShopActivity::Instalments => "instalments",
            ShopActivity::Optics => "optics",
            ShopActivity::Pressing => "pressing",
            ShopActivity::Salon => "salon",
            ShopActivity::Jewellery => "jewellery",
            ShopActivity::Gym => "gym",
            ShopActivity::Kiosk => "kiosk",
            ShopActivity::Containers => "containers",
            ShopActivity::Importer => "importer",
            ShopActivity::VanSales => "van_sales",
            ShopActivity::Workshop => "workshop",
            ShopActivity::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<ShopActivity> {
        ShopActivity::ALL.into_iter().find(|a| a.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Company {
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub legal_form: Option<String>,
    pub capital: Option<Money>,
    pub address: Option<String>,
    pub wilaya: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub rc: Option<String>,
    pub nif: Option<String>,
    pub nis: Option<String>,
    pub ai: Option<String>,
    pub rib: Option<String>,
    pub tax_regime: TaxRegime,
    pub price_mode: PriceMode,
    pub receipt_header_ar: Option<String>,
    pub receipt_header_fr: Option<String>,
    pub receipt_header_en: Option<String>,
    pub receipt_footer_ar: Option<String>,
    pub receipt_footer_fr: Option<String>,
    pub receipt_footer_en: Option<String>,
    pub default_doc_lang: Lang,
    pub has_logo: bool,
    pub updated_at: i64,
    /// The demo shop's database (`demo::META_DEMO`, never saved with the row): its names keep
    /// "DÉMO · ", it takes no fiscal identifier, and every receipt, document and label printed
    /// from it says "DÉMO – sans valeur fiscale".
    #[serde(default)]
    pub demo: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CompanyInput {
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub legal_form: Option<String>,
    pub capital: Option<Money>,
    pub address: Option<String>,
    pub wilaya: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub rc: Option<String>,
    pub nif: Option<String>,
    pub nis: Option<String>,
    pub ai: Option<String>,
    pub rib: Option<String>,
    pub tax_regime: TaxRegime,
    pub price_mode: PriceMode,
    pub receipt_header_ar: Option<String>,
    pub receipt_header_fr: Option<String>,
    pub receipt_header_en: Option<String>,
    pub receipt_footer_ar: Option<String>,
    pub receipt_footer_fr: Option<String>,
    pub receipt_footer_en: Option<String>,
    pub default_doc_lang: Lang,
}

impl CompanyInput {
    /// A company known only by its name in one language (first-run wizard).
    pub fn named(lang: Lang, name: &str) -> CompanyInput {
        let mut input = CompanyInput {
            default_doc_lang: lang,
            ..CompanyInput::default()
        };
        match lang {
            Lang::Ar => input.name_ar = name.to_owned(),
            Lang::Fr => input.name_fr = name.to_owned(),
            Lang::En => input.name_en = name.to_owned(),
        }
        input
    }
}

/// The stored company as the input that saves it again (`has_logo` and `updated_at` dropped).
impl From<Company> for CompanyInput {
    fn from(c: Company) -> CompanyInput {
        CompanyInput {
            name_ar: c.name_ar,
            name_fr: c.name_fr,
            name_en: c.name_en,
            legal_form: c.legal_form,
            capital: c.capital,
            address: c.address,
            wilaya: c.wilaya,
            phone: c.phone,
            email: c.email,
            rc: c.rc,
            nif: c.nif,
            nis: c.nis,
            ai: c.ai,
            rib: c.rib,
            tax_regime: c.tax_regime,
            price_mode: c.price_mode,
            receipt_header_ar: c.receipt_header_ar,
            receipt_header_fr: c.receipt_header_fr,
            receipt_header_en: c.receipt_header_en,
            receipt_footer_ar: c.receipt_footer_ar,
            receipt_footer_fr: c.receipt_footer_fr,
            receipt_footer_en: c.receipt_footer_en,
            default_doc_lang: c.default_doc_lang,
        }
    }
}

const COMPANY_SELECT: &str = "SELECT name_ar, name_fr, name_en, legal_form, capital, address, wilaya, phone,
        email, rc, nif, nis, ai, rib, tax_regime, price_mode, receipt_header_ar, receipt_header_fr,
        receipt_header_en, receipt_footer_ar, receipt_footer_fr, receipt_footer_en, default_doc_lang,
        logo IS NOT NULL, updated_at
     FROM company WHERE id = 1";

pub fn get(conn: &Connection) -> Result<Option<Company>, AppError> {
    let company = conn
        .query_row(COMPANY_SELECT, [], |r| {
            Ok(Company {
                name_ar: r.get(0)?,
                name_fr: r.get(1)?,
                name_en: r.get(2)?,
                legal_form: r.get(3)?,
                capital: r.get(4)?,
                address: r.get(5)?,
                wilaya: r.get(6)?,
                phone: r.get(7)?,
                email: r.get(8)?,
                rc: r.get(9)?,
                nif: r.get(10)?,
                nis: r.get(11)?,
                ai: r.get(12)?,
                rib: r.get(13)?,
                tax_regime: r.get(14)?,
                price_mode: r.get(15)?,
                receipt_header_ar: r.get(16)?,
                receipt_header_fr: r.get(17)?,
                receipt_header_en: r.get(18)?,
                receipt_footer_ar: r.get(19)?,
                receipt_footer_fr: r.get(20)?,
                receipt_footer_en: r.get(21)?,
                default_doc_lang: r.get(22)?,
                has_logo: r.get(23)?,
                updated_at: r.get(24)?,
                demo: false,
            })
        })
        .optional()?;
    let demo = crate::demo::is_marked(conn)?;
    Ok(company.map(|c| Company { demo, ..c }))
}

/// The shop's name in `lang`, else in another language it has; empty before the setup.
pub fn name_in(conn: &Connection, lang: Lang) -> Result<String, AppError> {
    Ok(get(conn)?
        .and_then(|c| {
            let names = match lang {
                Lang::Ar => [c.name_ar, c.name_fr, c.name_en],
                Lang::Fr => [c.name_fr, c.name_ar, c.name_en],
                Lang::En => [c.name_en, c.name_fr, c.name_ar],
            };
            names.into_iter().find(|n| !n.is_empty())
        })
        .unwrap_or_default())
}

/// The stored logo (PNG or JPEG), for the receipt header.
pub fn logo(conn: &Connection) -> Result<Option<Vec<u8>>, AppError> {
    Ok(conn
        .query_row("SELECT logo FROM company WHERE id = 1", [], |r| {
            r.get::<_, Option<Vec<u8>>>(0)
        })
        .optional()?
        .flatten())
}

/// NIF: digits only, with one of the data pack's lengths (`[15, 18]`; spaces removed before
/// the check). The database has no NIF constraint, so a new pack can change the rule.
pub fn is_valid_nif(nif: &str, lengths: &[u32]) -> bool {
    u32::try_from(nif.len()).is_ok_and(|len| lengths.contains(&len))
        && nif.bytes().all(|b| b.is_ascii_digit())
}

fn is_plausible_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.contains(char::is_whitespace)
}

fn opt(value: Option<String>, field: &str) -> Result<Option<String>, AppError> {
    let value = value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    if value.as_ref().is_some_and(|v| v.chars().count() > TEXT_MAX) {
        return Err(AppError::validation(field, "too_long"));
    }
    Ok(value)
}

fn clean(input: CompanyInput, pack: &Pack) -> Result<CompanyInput, AppError> {
    let name_ar = input.name_ar.trim().to_owned();
    let name_fr = input.name_fr.trim().to_owned();
    let name_en = input.name_en.trim().to_owned();
    if name_ar.is_empty() && name_fr.is_empty() && name_en.is_empty() {
        return Err(AppError::validation("name", "required"));
    }
    for (field, name) in [
        ("nameAr", &name_ar),
        ("nameFr", &name_fr),
        ("nameEn", &name_en),
    ] {
        if name.chars().count() > NAME_MAX {
            return Err(AppError::validation(field, "too_long"));
        }
    }
    let nif = opt(input.nif, "nif")?
        .map(|n| n.chars().filter(|c| !c.is_whitespace()).collect::<String>());
    if nif
        .as_deref()
        .is_some_and(|n| !is_valid_nif(n, &pack.nif_lengths))
    {
        return Err(AppError::validation("nif", "nif_invalid"));
    }
    // The NIS follows the NIF's digit rule (CFG-004): same lengths from the data pack.
    let nis = opt(input.nis, "nis")?
        .map(|n| n.chars().filter(|c| !c.is_whitespace()).collect::<String>());
    if nis
        .as_deref()
        .is_some_and(|n| !is_valid_nif(n, &pack.nif_lengths))
    {
        return Err(AppError::validation("nis", "nis_invalid"));
    }
    let email = opt(input.email, "email")?;
    if email.as_deref().is_some_and(|e| !is_plausible_email(e)) {
        return Err(AppError::validation("email", "email_invalid"));
    }
    let wilaya = opt(input.wilaya, "wilaya")?;
    if wilaya.as_deref().is_some_and(|w| pack.wilaya(w).is_none()) {
        return Err(AppError::validation("wilaya", "wilaya_invalid"));
    }
    if input.capital.is_some_and(Money::is_negative) {
        return Err(AppError::validation("capital", "amount_invalid"));
    }
    Ok(CompanyInput {
        name_ar,
        name_fr,
        name_en,
        legal_form: opt(input.legal_form, "legalForm")?,
        capital: input.capital,
        address: opt(input.address, "address")?,
        wilaya,
        phone: opt(input.phone, "phone")?,
        email,
        rc: opt(input.rc, "rc")?,
        nif,
        nis,
        ai: opt(input.ai, "ai")?,
        rib: opt(input.rib, "rib")?,
        tax_regime: input.tax_regime,
        price_mode: input.price_mode,
        receipt_header_ar: opt(input.receipt_header_ar, "receiptHeaderAr")?,
        receipt_header_fr: opt(input.receipt_header_fr, "receiptHeaderFr")?,
        receipt_header_en: opt(input.receipt_header_en, "receiptHeaderEn")?,
        receipt_footer_ar: opt(input.receipt_footer_ar, "receiptFooterAr")?,
        receipt_footer_fr: opt(input.receipt_footer_fr, "receiptFooterFr")?,
        receipt_footer_en: opt(input.receipt_footer_en, "receiptFooterEn")?,
        default_doc_lang: input.default_doc_lang,
    })
}

/// Whether prices are already in use: a product has a price (default level or another level),
/// or a document was numbered. The Z counter never restarts and numbers nothing sold.
fn prices_in_use(conn: &Connection) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM products WHERE sell_price <> 0)
             OR EXISTS (SELECT 1 FROM product_prices)
             OR EXISTS (SELECT 1 FROM counters WHERE doc_type <> ?1)",
        [crate::numbering::Z_COUNTER],
        |r| r.get(0),
    )?)
}

/// Creates or replaces the company row, validated against the data pack, and audits it as
/// `{ before, after }` snapshots (`before` is `null` on the first save). The price mode is fixed
/// once prices are in use (`price_mode_locked`): every stored price is read in that mode, so a
/// switch would move every till price by the VAT rate without a trace.
pub fn save(
    tx: &Transaction<'_>,
    actor: &Actor,
    pack: &Pack,
    input: CompanyInput,
) -> Result<Company, AppError> {
    let c = clean(input, pack)?;
    if crate::demo::is_marked(tx)? {
        crate::demo::check_company(&c)?;
    }
    let before = get(tx)?;
    if before
        .as_ref()
        .is_some_and(|current| current.price_mode != c.price_mode)
        && prices_in_use(tx)?
    {
        return Err(AppError::validation("priceMode", "price_mode_locked"));
    }
    tx.execute(
        "INSERT INTO company (id, name_ar, name_fr, name_en, legal_form, capital, address, wilaya, phone,
             email, rc, nif, nis, ai, rib, tax_regime, price_mode, receipt_header_ar, receipt_header_fr,
             receipt_header_en, receipt_footer_ar, receipt_footer_fr, receipt_footer_en, default_doc_lang,
             updated_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19,
             ?20, ?21, ?22, ?23, ?24)
         ON CONFLICT (id) DO UPDATE SET
             name_ar = excluded.name_ar, name_fr = excluded.name_fr, name_en = excluded.name_en,
             legal_form = excluded.legal_form, capital = excluded.capital, address = excluded.address,
             wilaya = excluded.wilaya, phone = excluded.phone, email = excluded.email, rc = excluded.rc,
             nif = excluded.nif, nis = excluded.nis, ai = excluded.ai, rib = excluded.rib,
             tax_regime = excluded.tax_regime, price_mode = excluded.price_mode,
             receipt_header_ar = excluded.receipt_header_ar, receipt_header_fr = excluded.receipt_header_fr,
             receipt_header_en = excluded.receipt_header_en, receipt_footer_ar = excluded.receipt_footer_ar,
             receipt_footer_fr = excluded.receipt_footer_fr, receipt_footer_en = excluded.receipt_footer_en,
             default_doc_lang = excluded.default_doc_lang, updated_at = excluded.updated_at",
        params![
            c.name_ar,
            c.name_fr,
            c.name_en,
            c.legal_form,
            c.capital,
            c.address,
            c.wilaya,
            c.phone,
            c.email,
            c.rc,
            c.nif,
            c.nis,
            c.ai,
            c.rib,
            c.tax_regime,
            c.price_mode,
            c.receipt_header_ar,
            c.receipt_header_fr,
            c.receipt_header_en,
            c.receipt_footer_ar,
            c.receipt_footer_fr,
            c.receipt_footer_en,
            c.default_doc_lang,
            now_ms()
        ],
    )?;
    let after =
        get(tx)?.ok_or_else(|| AppError::Internal("company row missing after save".into()))?;
    let payload =
        json!({ "before": serde_json::to_value(&before)?, "after": serde_json::to_value(&after)? });
    audit::append(
        tx,
        &actor.event("company.updated", "company", Some("1"), payload),
    )?;
    Ok(after)
}

/// The largest logo accepted: printed small on tickets and invoices, it never needs more.
pub const LOGO_MAX_BYTES: usize = 512 * 1024;

/// `png` or `jpeg`, told by the file's signature (never by its name).
fn logo_format(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpeg")
    } else {
        None
    }
}

/// Stores the shop logo as is (a PNG or JPEG of at most [`LOGO_MAX_BYTES`]); `None` removes it.
/// The company row must exist (the first-run setup creates it). Audited with the format and
/// size, never the image itself.
pub fn set_logo(
    tx: &Transaction<'_>,
    actor: &Actor,
    logo: Option<&[u8]>,
) -> Result<Company, AppError> {
    let format = match logo {
        Some(bytes) if bytes.len() > LOGO_MAX_BYTES => {
            return Err(AppError::validation("logo", "logo_too_large"));
        }
        Some(bytes) => {
            Some(logo_format(bytes).ok_or_else(|| AppError::validation("logo", "logo_invalid"))?)
        }
        None => None,
    };
    let changed = tx.execute(
        "UPDATE company SET logo = ?1, updated_at = ?2 WHERE id = 1",
        params![logo, now_ms()],
    )?;
    if changed == 0 {
        return Err(AppError::not_found("record"));
    }
    audit::append(
        tx,
        &actor.event(
            "company.logo_changed",
            "company",
            Some("1"),
            json!({ "format": format, "size": logo.map_or(0, <[u8]>::len) }),
        ),
    )?;
    get(tx)?.ok_or_else(|| AppError::Internal("company row missing after the logo".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use serde_json::json;

    fn save_input(db: &Db, input: CompanyInput) -> Result<Company, AppError> {
        let pack = crate::datapack::bundled().unwrap();
        db.write(move |tx| save(tx, &Actor::default(), &pack, input))
    }

    fn error_of(db: &Db, input: CompanyInput) -> (String, String) {
        let payload = save_input(db, input).unwrap_err().payload();
        (payload.field.unwrap_or_default(), payload.code)
    }

    #[test]
    fn empty_database_has_no_company() {
        assert_eq!(Db::open_memory().unwrap().read(get).unwrap(), None);
    }

    #[test]
    fn save_normalises_and_audits() {
        let db = Db::open_memory().unwrap();
        let mut input = CompanyInput::named(Lang::Fr, "  Supérette El Amel ");
        input.nif = Some("0001 1600 1234 567".into());
        input.wilaya = Some("16".into());
        input.email = Some(" contact@elamel.dz ".into());
        input.capital = Some(Money(100_000_000));
        input.phone = Some("   ".into());
        let company = save_input(&db, input).unwrap();
        assert_eq!(company.name_fr, "Supérette El Amel");
        assert_eq!(company.nif.as_deref(), Some("000116001234567"));
        assert_eq!(company.email.as_deref(), Some("contact@elamel.dz"));
        assert_eq!(company.phone, None);
        assert_eq!(
            (
                company.tax_regime,
                company.price_mode,
                company.default_doc_lang
            ),
            (TaxRegime::Reel, PriceMode::Ttc, Lang::Fr)
        );
        assert!(!company.has_logo);
        let action: String = db
            .read(|c| Ok(c.query_row("SELECT action FROM audit_log", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(action, "company.updated");
    }

    #[test]
    fn saving_twice_keeps_one_row() {
        let db = Db::open_memory().unwrap();
        save_input(&db, CompanyInput::named(Lang::Ar, "مغازة النور")).unwrap();
        let mut second = CompanyInput::named(Lang::Ar, "مغازة الأمل");
        second.tax_regime = TaxRegime::Ifu;
        let company = save_input(&db, second).unwrap();
        assert_eq!(
            (company.name_ar.as_str(), company.tax_regime),
            ("مغازة الأمل", TaxRegime::Ifu)
        );
        let rows: i64 = db
            .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM company", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn validation_errors_name_the_field() {
        let db = Db::open_memory().unwrap();
        let base = CompanyInput::named(Lang::Ar, "مغازة النور");
        let with = |f: fn(&mut CompanyInput)| {
            let mut input = base.clone();
            f(&mut input);
            input
        };
        assert_eq!(
            error_of(&db, with(|c| c.nif = Some("12345".into()))),
            ("nif".into(), "nif_invalid".into())
        );
        assert_eq!(
            error_of(&db, with(|c| c.email = Some("not-an-email".into()))),
            ("email".into(), "email_invalid".into())
        );
        assert_eq!(
            error_of(&db, with(|c| c.wilaya = Some("99".into()))),
            ("wilaya".into(), "wilaya_invalid".into())
        );
        assert_eq!(
            error_of(&db, with(|c| c.capital = Some(Money(-1)))),
            ("capital".into(), "amount_invalid".into())
        );
        assert_eq!(
            error_of(&db, with(|c| c.name_ar = " ".into())),
            ("name".into(), "required".into())
        );
        assert_eq!(
            error_of(&db, with(|c| c.name_fr = "x".repeat(81))),
            ("nameFr".into(), "too_long".into())
        );
    }

    #[test]
    fn enums_serialise_as_stored() {
        for activity in ShopActivity::ALL {
            assert_eq!(ShopActivity::parse(activity.as_str()), Some(activity));
            assert_eq!(
                serde_json::to_value(activity).unwrap(),
                json!(activity.as_str())
            );
        }
        assert_eq!(serde_json::to_value(TaxRegime::Ifu).unwrap(), json!("ifu"));
        assert_eq!(serde_json::to_value(PriceMode::Ht).unwrap(), json!("ht"));
    }

    #[test]
    fn nif_is_15_or_18_digits_per_the_data_pack() {
        let lengths = crate::datapack::bundled().unwrap().nif_lengths;
        assert!(is_valid_nif("000116001234567", &lengths), "15 digits");
        assert!(
            is_valid_nif("000116001234567001", &lengths),
            "18 digits: secondary establishment"
        );
        assert!(!is_valid_nif("00011600123456", &lengths), "14 digits");
        assert!(!is_valid_nif("00011600123456A", &lengths), "a letter");
        let db = Db::open_memory().unwrap();
        let mut input = CompanyInput::named(Lang::Fr, "Supérette El Amel");
        input.nif = Some("0001 1600 1234 5670 01".into());
        assert_eq!(
            save_input(&db, input).unwrap().nif.as_deref(),
            Some("000116001234567001")
        );
    }

    fn audit_payloads(db: &Db) -> Vec<serde_json::Value> {
        let texts = db
            .read(|c| {
                let mut stmt = c.prepare("SELECT payload FROM audit_log ORDER BY seq")?;
                let texts = stmt
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(texts)
            })
            .unwrap();
        texts
            .iter()
            .map(|t| serde_json::from_str(t).unwrap())
            .collect()
    }

    #[test]
    fn every_save_audits_before_and_after() {
        let db = Db::open_memory().unwrap();
        let first = save_input(&db, CompanyInput::named(Lang::Fr, "Supérette El Amel")).unwrap();
        let mut input = CompanyInput::named(Lang::Fr, "Supérette El Amel");
        input.price_mode = PriceMode::Ht;
        let second = save_input(&db, input).unwrap();
        assert_eq!(
            audit_payloads(&db),
            [
                json!({ "before": null, "after": first }),
                json!({ "before": first, "after": second }),
            ]
        );
        let refused = save_input(&db, CompanyInput::named(Lang::Fr, " "));
        assert_eq!(refused.unwrap_err().code(), "required");
        assert_eq!(
            audit_payloads(&db).len(),
            2,
            "a refused save writes nothing"
        );
        assert_eq!(db.read(get).unwrap(), Some(second));
    }

    #[test]
    fn nis_follows_the_nif_digit_rule() {
        let db = Db::open_memory().unwrap();
        let mut input = CompanyInput::named(Lang::Fr, "Supérette El Amel");
        input.nis = Some("0981 1601 2345 678".into());
        assert_eq!(
            save_input(&db, input.clone()).unwrap().nis.as_deref(),
            Some("098116012345678")
        );
        input.nis = Some("098116012345678001".into());
        assert!(save_input(&db, input.clone()).is_ok(), "18 digits");
        for bad in ["12345", "09811601234567", "09811601234567A"] {
            input.nis = Some(bad.into());
            assert_eq!(
                error_of(&db, input.clone()),
                ("nis".into(), "nis_invalid".into()),
                "{bad}"
            );
        }
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F'];

    fn put_logo(db: &Db, logo: Option<Vec<u8>>) -> Result<Company, AppError> {
        db.write(move |tx| set_logo(tx, &Actor::default(), logo.as_deref()))
    }

    fn stored_logo(db: &Db) -> Option<Vec<u8>> {
        db.read(|c| Ok(c.query_row("SELECT logo FROM company WHERE id = 1", [], |r| r.get(0))?))
            .unwrap()
    }

    #[test]
    fn logo_is_a_png_or_jpeg_of_at_most_512_kb() {
        let db = Db::open_memory().unwrap();
        save_input(&db, CompanyInput::named(Lang::Fr, "Supérette El Amel")).unwrap();
        assert!(put_logo(&db, Some(PNG.to_vec())).unwrap().has_logo);
        assert_eq!(stored_logo(&db).as_deref(), Some(PNG));
        let mut largest = JPEG.to_vec();
        largest.resize(512 * 1024, 0);
        assert!(put_logo(&db, Some(largest.clone())).unwrap().has_logo);
        assert_eq!(stored_logo(&db).as_ref(), Some(&largest));
        let mut oversized = largest.clone();
        oversized.push(0);
        let refused = put_logo(&db, Some(oversized)).unwrap_err().payload();
        assert_eq!(
            (refused.field.as_deref(), refused.code.as_str()),
            (Some("logo"), "logo_too_large")
        );
        for bad in [Vec::new(), b"GIF89a".to_vec(), b"<svg xmlns".to_vec()] {
            let refused = put_logo(&db, Some(bad)).unwrap_err().payload();
            assert_eq!(
                (refused.field.as_deref(), refused.code.as_str()),
                (Some("logo"), "logo_invalid")
            );
        }
        assert_eq!(
            stored_logo(&db),
            Some(largest),
            "a refused logo keeps the current one"
        );
        assert!(!put_logo(&db, None).unwrap().has_logo, "null removes it");
        assert_eq!(stored_logo(&db), None);
    }

    #[test]
    fn logo_changes_are_audited_and_survive_an_identity_save() {
        let db = Db::open_memory().unwrap();
        save_input(&db, CompanyInput::named(Lang::Fr, "Supérette El Amel")).unwrap();
        put_logo(&db, Some(JPEG.to_vec())).unwrap();
        let saved = save_input(&db, CompanyInput::named(Lang::Fr, "Supérette El Amel")).unwrap();
        assert!(saved.has_logo, "saving the identity keeps the logo");
        assert_eq!(stored_logo(&db).as_deref(), Some(JPEG));
        put_logo(&db, None).unwrap();
        let actions: Vec<String> = db
            .read(|c| {
                let mut stmt = c.prepare("SELECT action FROM audit_log ORDER BY seq")?;
                let rows = stmt
                    .query_map([], |r| r.get(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        assert_eq!(
            actions,
            [
                "company.updated",
                "company.logo_changed",
                "company.updated",
                "company.logo_changed"
            ]
        );
        let payloads = audit_payloads(&db);
        assert_eq!(payloads[1], json!({ "format": "jpeg", "size": 10 }));
        assert_eq!(payloads[3], json!({ "format": null, "size": 0 }));
    }

    #[test]
    fn logo_needs_the_company_row() {
        let db = Db::open_memory().unwrap();
        assert_eq!(
            put_logo(&db, Some(PNG.to_vec())).unwrap_err().code(),
            "record_not_found"
        );
    }

    #[test]
    fn logo_bytes_are_read_back_for_printing() {
        let db = Db::open_memory().unwrap();
        assert_eq!(db.read(logo).unwrap(), None);
        save_input(&db, CompanyInput::named(Lang::Fr, "Supérette El Amel")).unwrap();
        assert_eq!(db.read(logo).unwrap(), None, "a company without a logo");
        put_logo(&db, Some(PNG.to_vec())).unwrap();
        assert_eq!(db.read(logo).unwrap().as_deref(), Some(PNG));
    }

    /// Stored prices are read in the price mode: once one exists, the mode stays.
    #[test]
    fn the_price_mode_is_fixed_once_prices_are_in_use() {
        let shop = crate::testkit::shop();
        let with_mode = |mode: PriceMode| {
            let mut input = CompanyInput::named(Lang::Fr, "Supérette El Amel");
            input.nif = Some("000116001234567".into());
            input.price_mode = mode;
            let pack = shop.pack.clone();
            shop.db
                .write(move |tx| save(tx, &Actor::default(), &pack, input))
        };
        // No price yet: the owner may still change their mind.
        assert_eq!(with_mode(PriceMode::Ht).unwrap().price_mode, PriceMode::Ht);
        assert_eq!(
            with_mode(PriceMode::Ttc).unwrap().price_mode,
            PriceMode::Ttc
        );
        shop.product("Lait", Money(10_000), &shop.tva19);
        let err = with_mode(PriceMode::Ht).unwrap_err();
        assert_eq!(
            (err.code(), err.payload().field),
            ("price_mode_locked".into(), Some("priceMode".into()))
        );
        // Everything else can still change.
        assert!(with_mode(PriceMode::Ttc).is_ok());
    }
}
