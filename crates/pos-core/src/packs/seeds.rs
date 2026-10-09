//! What a template creates: categories (by French name, case-insensitive: an existing one is
//! kept), service products (by `SVC-…` reference), the receipt footer when the shop has none, and
//! the modules' own seeds. Each record is created through its own service (and journaled there).

use super::Module;
use super::templates::{PackTemplate, SeedService};
use crate::audit::Actor;
use crate::catalog::categories::{self, CategoryInput};
use crate::catalog::products::{self, ProductInput, ProductKind};
use crate::catalog::tax_rates;
use crate::company::ShopActivity;
use crate::datapack::Pack;
use crate::error::AppError;
use crate::lang::Lang;
use crate::money::Money;
use crate::time::now_ms;
use rusqlite::{OptionalExtension, Transaction, params};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Seeded {
    pub categories: u32,
    pub services: u32,
    pub extra: u32,
}

pub fn run(
    tx: &Transaction<'_>,
    actor: &Actor,
    pack: &Pack,
    template: &PackTemplate,
    _lang: Lang,
) -> Result<Seeded, AppError> {
    crate::catalog::ensure_defaults(tx, pack)?;
    let mut out = Seeded::default();
    for (i, name) in template.categories.iter().enumerate() {
        let exists: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM categories
                            WHERE archived_at IS NULL AND lower(name_fr) = lower(?1))",
            [name.fr],
            |r| r.get(0),
        )?;
        if exists {
            continue;
        }
        let input = CategoryInput {
            parent_id: None,
            name_ar: name.ar.to_owned(),
            name_fr: name.fr.to_owned(),
            name_en: name.en.to_owned(),
            color: None,
            sort_order: i64::try_from(i).unwrap_or(0) + 1,
            image: None,
        };
        categories::create(tx, actor, input)?;
        out.categories += 1;
    }
    for service in template.services {
        if service_product(tx, actor, service)?.is_some() {
            out.services += 1;
        }
    }
    seed_footer(tx, template)?;
    for module in template.modules {
        out.extra += for_module(tx, actor, pack, template.activity, *module)?;
    }
    Ok(out)
}

/// Creates the service product of `service` unless a product already carries its reference;
/// the id when created.
pub(crate) fn service_product(
    tx: &Transaction<'_>,
    actor: &Actor,
    service: &SeedService,
) -> Result<Option<String>, AppError> {
    service_product_named(
        tx,
        actor,
        service.code,
        (service.name.ar, service.name.fr, service.name.en),
        service.unit_id,
        service.tax_code,
        Money(service.price),
        service.open_price,
    )
}

/// `service_product` with owned names (names built at run time, e.g. from the data pack).
#[allow(clippy::too_many_arguments)]
pub(crate) fn service_product_named(
    tx: &Transaction<'_>,
    actor: &Actor,
    code: &str,
    names: (&str, &str, &str),
    unit_id: &str,
    tax_code: &str,
    price: Money,
    open_price: bool,
) -> Result<Option<String>, AppError> {
    let existing: Option<String> = tx
        .query_row(
            "SELECT id FROM products WHERE reference = ?1",
            [code],
            |r| r.get(0),
        )
        .optional()?;
    if existing.is_some() {
        return Ok(None);
    }
    let rate = tax_rates::by_code(tx, tax_code)?.ok_or_else(|| AppError::not_found("tax_rate"))?;
    let mut input = ProductInput::simple(names.1, unit_id, &rate.id, price);
    input.reference = Some(code.to_owned());
    input.name_ar = names.0.to_owned();
    input.name_en = names.2.to_owned();
    input.kind = ProductKind::Service;
    input.track_stock = false;
    let product = products::create(tx, actor, input)?;
    if open_price {
        tx.execute(
            "INSERT INTO open_price_products (product_id, created_at) VALUES (?1, ?2)",
            params![product.id, now_ms()],
        )?;
    }
    Ok(Some(product.id))
}

/// Writes the template's footer when the shop has none in any language (the owner's text always
/// wins). The template's `packs.template_applied` row journals it.
fn seed_footer(tx: &Transaction<'_>, template: &PackTemplate) -> Result<(), AppError> {
    let Some(footer) = template.footer else {
        return Ok(());
    };
    tx.execute(
        "UPDATE company SET receipt_footer_ar = ?1, receipt_footer_fr = ?2, receipt_footer_en = ?3,
                            updated_at = ?4
         WHERE id = 1 AND trim(COALESCE(receipt_footer_ar, '')) = ''
           AND trim(COALESCE(receipt_footer_fr, '')) = '' AND trim(COALESCE(receipt_footer_en, '')) = ''",
        params![footer.ar, footer.fr, footer.en, now_ms()],
    )?;
    Ok(())
}

/// The module's own seeds for `activity`; the number of records created.
pub(crate) fn for_module(
    tx: &Transaction<'_>,
    actor: &Actor,
    pack: &Pack,
    activity: ShopActivity,
    module: Module,
) -> Result<u32, AppError> {
    match module {
        // VRT-001: the superette ships the regulated staples at their ceilings.
        Module::RegulatedPrices if activity == ShopActivity::General => {
            crate::regulated::seed_staples(tx, actor, pack)
        }
        Module::DepartmentKeys => crate::catalog::departments::seed(tx, actor, activity),
        Module::Restaurant => crate::restaurant::seed(tx, actor),
        // VRT-002: one open-price top-up product per mobile operator of the data pack.
        Module::Flexy => crate::flexy::seed(tx, actor, pack),
        // TMP-002: the instalment price level.
        Module::Instalments => crate::instalments::seed_level(tx, actor),
        // ORD-003: the "Arrhes conservées" service a kept deposit is sold as.
        Module::Orders => crate::orders::kept_deposit_product(tx, actor).map(|_| 1),
        // Modules without seeds of their own.
        _ => Ok(0),
    }
}
