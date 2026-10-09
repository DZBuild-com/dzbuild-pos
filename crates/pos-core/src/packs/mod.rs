//! Vertical packs (master spec §30, PLT-016, ONB-001, ONB-008, UX-020): an activity template
//! switches modules on, picks the till mode and seeds categories, service lines and receipt texts
//! in Arabic, French and English. A module is a local switch of the owner: a pack feature refuses
//! its writes while its module is off (`module_disabled`), and its data always stays.

pub mod seeds;
pub mod templates;
#[cfg(test)]
mod tests;

use crate::audit::{self, Actor};
use crate::company::ShopActivity;
use crate::datapack::Pack;
use crate::error::AppError;
use crate::lang::Lang;
use crate::settings;
use rusqlite::{Connection, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use specta::Type;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum Module {
    Restaurant,
    DepartmentKeys,
    RegulatedPrices,
    Lots,
    VariantGrid,
    Yield,
    Waste,
    Pharmacy,
    Serials,
    Flexy,
    Repairs,
    Instalments,
    Wholesale,
    DeliveryRounds,
    Orders,
    Appointments,
    Loyalty,
    GiftVouchers,
    Jewellery,
    Staff,
    Zakat,
}

impl Module {
    pub const ALL: [Module; 21] = [
        Module::Restaurant,
        Module::DepartmentKeys,
        Module::RegulatedPrices,
        Module::Lots,
        Module::VariantGrid,
        Module::Yield,
        Module::Waste,
        Module::Pharmacy,
        Module::Serials,
        Module::Flexy,
        Module::Repairs,
        Module::Instalments,
        Module::Wholesale,
        Module::DeliveryRounds,
        Module::Orders,
        Module::Appointments,
        Module::Loyalty,
        Module::GiftVouchers,
        Module::Jewellery,
        Module::Staff,
        Module::Zakat,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Module::Restaurant => "restaurant",
            Module::DepartmentKeys => "department_keys",
            Module::RegulatedPrices => "regulated_prices",
            Module::Lots => "lots",
            Module::VariantGrid => "variant_grid",
            Module::Yield => "yield",
            Module::Waste => "waste",
            Module::Pharmacy => "pharmacy",
            Module::Serials => "serials",
            Module::Flexy => "flexy",
            Module::Repairs => "repairs",
            Module::Instalments => "instalments",
            Module::Wholesale => "wholesale",
            Module::DeliveryRounds => "delivery_rounds",
            Module::Orders => "orders",
            Module::Appointments => "appointments",
            Module::Loyalty => "loyalty",
            Module::GiftVouchers => "gift_vouchers",
            Module::Jewellery => "jewellery",
            Module::Staff => "staff",
            Module::Zakat => "zakat",
        }
    }

    pub fn parse(s: &str) -> Option<Module> {
        Module::ALL.into_iter().find(|m| m.as_str() == s)
    }
}

/// How the sale screen opens (UX-020): the retail grid, the restaurant floor, the customer-first
/// wholesale till, or the retail grid with a floor button (café + épicerie).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TillMode {
    Retail,
    Restaurant,
    Wholesale,
    Mixed,
}

impl TillMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            TillMode::Retail => "retail",
            TillMode::Restaurant => "restaurant",
            TillMode::Wholesale => "wholesale",
            TillMode::Mixed => "mixed",
        }
    }

    pub fn parse(s: &str) -> Option<TillMode> {
        [
            TillMode::Retail,
            TillMode::Restaurant,
            TillMode::Wholesale,
            TillMode::Mixed,
        ]
        .into_iter()
        .find(|m| m.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PackState {
    pub activity: ShopActivity,
    pub modules: Vec<Module>,
    pub till_mode: TillMode,
    /// What the activity's template switches on (the settings screen shows the difference).
    pub template_modules: Vec<Module>,
    /// The owner chose the modules by hand (`packs.modules` is set).
    pub customised: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub activity: ShopActivity,
    pub categories: u32,
    pub services: u32,
    /// Records the modules' own seeds created (department keys, regulated staples, dining room…).
    pub extra: u32,
}

pub fn state(conn: &Connection) -> Result<PackState, AppError> {
    let activity: ShopActivity = settings::get(conn, "shop.activity")?;
    let template = templates::template(activity);
    let stored = settings::get_value(conn, "packs.modules")?;
    let customised = !stored.is_null();
    let mut modules: Vec<Module> = match stored.as_array() {
        Some(items) => items
            .iter()
            .filter_map(|m| m.as_str().and_then(Module::parse))
            .collect(),
        None => template.modules.to_vec(),
    };
    modules.sort();
    let mut template_modules = template.modules.to_vec();
    template_modules.sort();
    let till_mode = settings::get_value(conn, "packs.till_mode")?
        .as_str()
        .and_then(TillMode::parse)
        .unwrap_or(template.till_mode);
    Ok(PackState {
        activity,
        modules,
        till_mode,
        template_modules,
        customised,
    })
}

pub fn enabled(conn: &Connection, module: Module) -> Result<bool, AppError> {
    Ok(state(conn)?.modules.contains(&module))
}

/// Refuses a pack write while its module is off.
pub fn require(conn: &Connection, module: Module) -> Result<(), AppError> {
    if enabled(conn, module)? {
        Ok(())
    } else {
        Err(AppError::rejected(
            json!({ "module": module.as_str() }),
            "module_disabled",
        ))
    }
}

fn modules_value(modules: &[Module]) -> Value {
    let mut sorted = modules.to_vec();
    sorted.sort();
    sorted.dedup();
    Value::Array(sorted.iter().map(|m| json!(m.as_str())).collect())
}

pub fn set_modules(
    tx: &Transaction<'_>,
    actor: &Actor,
    modules: &[Module],
) -> Result<PackState, AppError> {
    let before = state(tx)?;
    let value = settings::set_quiet(tx, "packs.modules", modules_value(modules))?;
    audit::append(
        tx,
        &actor.event(
            "packs.modules_set",
            "setting",
            Some("packs.modules"),
            json!({ "before": modules_value(&before.modules), "after": value }),
        ),
    )?;
    state(tx)
}

/// [`set_modules`] from the settings screen (ONB-008): each module the owner turns on also gets
/// the module's own seeds its template gives it (the flexy operators' top-up products, the
/// instalment price level, a first dining room…), so it works at once. Every seed is created
/// once (a module turned off and on again, or one that stays on, adds nothing); the template's
/// categories and services are not touched.
pub fn switch_modules(
    tx: &Transaction<'_>,
    actor: &Actor,
    pack: &Pack,
    modules: &[Module],
) -> Result<PackState, AppError> {
    let before = state(tx)?.modules;
    let after = set_modules(tx, actor, modules)?;
    for module in after.modules.iter().filter(|m| !before.contains(m)) {
        seeds::for_module(tx, actor, pack, after.activity, *module)?;
    }
    Ok(after)
}

/// Back to the template's modules.
pub fn reset_modules(tx: &Transaction<'_>, actor: &Actor) -> Result<PackState, AppError> {
    let before = state(tx)?;
    settings::set_quiet(tx, "packs.modules", Value::Null)?;
    audit::append(
        tx,
        &actor.event(
            "packs.modules_set",
            "setting",
            Some("packs.modules"),
            json!({ "before": modules_value(&before.modules), "after": Value::Null }),
        ),
    )?;
    state(tx)
}

pub fn set_till_mode(
    tx: &Transaction<'_>,
    actor: &Actor,
    mode: TillMode,
) -> Result<PackState, AppError> {
    let before = state(tx)?.till_mode;
    settings::set_quiet(tx, "packs.till_mode", json!(mode.as_str()))?;
    audit::append(
        tx,
        &actor.event(
            "packs.till_mode_set",
            "setting",
            Some("packs.till_mode"),
            json!({ "before": before.as_str(), "after": mode.as_str() }),
        ),
    )?;
    state(tx)
}

/// Makes `activity` the shop's template (ONB-008): the activity, the template's modules (the
/// owner's hand-picked list is reset), its till mode, and — with `seed` — its categories, service
/// lines, receipt footer and the modules' own seeds, none of which is created twice. Journaled
/// once as `packs.template_applied`; the records it creates are journaled by their services.
pub fn apply_template(
    tx: &Transaction<'_>,
    actor: &Actor,
    pack: &Pack,
    activity: ShopActivity,
    lang: Lang,
    seed: bool,
) -> Result<ApplyReport, AppError> {
    let template = templates::template(activity);
    let before: ShopActivity = settings::get(tx, "shop.activity")?;
    settings::set_quiet(tx, "shop.activity", json!(activity))?;
    settings::set_quiet(tx, "packs.modules", Value::Null)?;
    settings::set_quiet(tx, "packs.till_mode", Value::Null)?;
    if template.modules.contains(&Module::Restaurant) {
        crate::restaurant::seed_settings(tx, lang)?;
    }
    let seeded = if seed {
        seeds::run(tx, actor, pack, template, lang)?
    } else {
        seeds::Seeded::default()
    };
    let report = ApplyReport {
        activity,
        categories: seeded.categories,
        services: seeded.services,
        extra: seeded.extra,
    };
    audit::append(
        tx,
        &actor.event(
            "packs.template_applied",
            "setting",
            Some("shop.activity"),
            json!({
                "before": before,
                "after": activity,
                "seed": seed,
                "categories": report.categories,
                "services": report.services,
                "extra": report.extra,
            }),
        ),
    )?;
    Ok(report)
}
