//! Modifiers (PRD-021, SAL-024): groups of options attached to products, with a minimum (a forced
//! choice when ≥ 1) and a maximum (0 = no maximum). An option without a product is written on the
//! line as a note in the ticket's language ("Harissa, bien cuit"); an option with a product
//! ("Supplément fromage") is sold as its own line of that product, so its price, TVA and stock
//! follow the ordinary sale rules and the kitchen ticket prints it under its dish.

use crate::audit::{self, Actor};
use crate::catalog::{clean_names, products};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::time::now_ms;
use rusqlite::{Connection, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModifierOption {
    pub id: String,
    pub group_id: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub product_id: Option<String>,
    pub sort_order: i64,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModifierGroup {
    pub id: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub min_select: u8,
    pub max_select: u8,
    pub sort_order: i64,
    pub archived: bool,
    pub options: Vec<ModifierOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModifierOptionInput {
    /// An existing option to keep (its id stays, so past lines still name it).
    pub id: Option<String>,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub product_id: Option<String>,
    pub sort_order: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModifierGroupInput {
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub min_select: u8,
    pub max_select: u8,
    pub sort_order: i64,
    pub options: Vec<ModifierOptionInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppliedOption {
    pub option_id: String,
    pub product_id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    /// The unpriced options, joined, in the ticket's language.
    pub note: Option<String>,
    pub options: Vec<AppliedOption>,
}

fn map_option(r: &Row<'_>) -> rusqlite::Result<ModifierOption> {
    Ok(ModifierOption {
        id: r.get(0)?,
        group_id: r.get(1)?,
        name_ar: r.get(2)?,
        name_fr: r.get(3)?,
        name_en: r.get(4)?,
        product_id: r.get(5)?,
        sort_order: r.get(6)?,
        archived: r.get(7)?,
    })
}

fn load(conn: &Connection, where_: &str, arg: &str) -> Result<Vec<ModifierGroup>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT g.id, g.name_ar, g.name_fr, g.name_en, g.min_select, g.max_select, g.sort_order,
                g.archived_at IS NOT NULL FROM modifier_groups g {where_}"
    ))?;
    let mut groups = stmt
        .query_map([arg], |r| {
            Ok(ModifierGroup {
                id: r.get(0)?,
                name_ar: r.get(1)?,
                name_fr: r.get(2)?,
                name_en: r.get(3)?,
                min_select: r.get(4)?,
                max_select: r.get(5)?,
                sort_order: r.get(6)?,
                archived: r.get(7)?,
                options: Vec::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut opts = conn.prepare_cached(
        "SELECT id, group_id, name_ar, name_fr, name_en, product_id, sort_order, archived_at IS NOT NULL
         FROM modifier_options WHERE group_id = ?1 AND archived_at IS NULL ORDER BY sort_order, name_fr",
    )?;
    for g in &mut groups {
        g.options = opts
            .query_map([&g.id], map_option)?
            .collect::<Result<Vec<_>, _>>()?;
    }
    Ok(groups)
}

pub fn groups(conn: &Connection, include_archived: bool) -> Result<Vec<ModifierGroup>, AppError> {
    load(
        conn,
        "WHERE (?1 = 'all' OR g.archived_at IS NULL) ORDER BY g.sort_order, g.name_fr",
        if include_archived { "all" } else { "live" },
    )
}

fn group(conn: &Connection, id: &str) -> Result<ModifierGroup, AppError> {
    load(conn, "WHERE g.id = ?1", id)?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::not_found("modifier_group"))
}

/// The live groups of a product, in their order on the product.
pub fn for_product(conn: &Connection, product_id: &str) -> Result<Vec<ModifierGroup>, AppError> {
    load(
        conn,
        "JOIN product_modifier_groups pg ON pg.group_id = g.id
         WHERE pg.product_id = ?1 AND g.archived_at IS NULL ORDER BY pg.sort_order, g.sort_order",
        product_id,
    )
}

pub fn save_group(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: Option<&str>,
    input: ModifierGroupInput,
) -> Result<ModifierGroup, AppError> {
    let (ar, fr, en) = clean_names(&input.name_ar, &input.name_fr, &input.name_en)?;
    if input.min_select > 10
        || input.max_select > 10
        || (input.max_select > 0 && input.max_select < input.min_select)
    {
        return Err(AppError::validation("maxSelect", "value_invalid"));
    }
    if input.options.is_empty() || input.options.len() > 40 {
        return Err(AppError::validation("options", "value_invalid"));
    }
    // A forced choice the options cannot fill would make the product impossible to order.
    if usize::from(input.min_select) > input.options.len() {
        return Err(AppError::validation("minSelect", "value_invalid"));
    }
    let now = now_ms();
    let id = match id {
        Some(id) => {
            group(tx, id)?;
            tx.execute(
                "UPDATE modifier_groups SET name_ar = ?2, name_fr = ?3, name_en = ?4, min_select = ?5,
                                            max_select = ?6, sort_order = ?7, updated_at = ?8 WHERE id = ?1",
                params![id, ar, fr, en, input.min_select, input.max_select, input.sort_order, now],
            )?;
            id.to_owned()
        }
        None => {
            let id = new_id();
            tx.execute(
                "INSERT INTO modifier_groups (id, name_ar, name_fr, name_en, min_select, max_select, sort_order,
                                              created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                params![id, ar, fr, en, input.min_select, input.max_select, input.sort_order, now],
            )?;
            id
        }
    };
    let mut kept: Vec<String> = Vec::new();
    for (i, o) in input.options.iter().enumerate() {
        let (oar, ofr, oen) =
            clean_names(&o.name_ar, &o.name_fr, &o.name_en).map_err(|e| match e {
                AppError::Validation { field, code } => {
                    AppError::validation(&format!("options.{i}.{field}"), &code)
                }
                other => other,
            })?;
        if let Some(pid) = &o.product_id {
            products::sellable(tx, pid).map_err(|_| {
                AppError::validation(&format!("options.{i}.productId"), "product_not_found")
            })?;
        }
        let option_id = match &o.id {
            Some(existing) => {
                let changed = tx.execute(
                    "UPDATE modifier_options SET name_ar = ?3, name_fr = ?4, name_en = ?5, product_id = ?6,
                                                 sort_order = ?7, archived_at = NULL, updated_at = ?8
                     WHERE id = ?1 AND group_id = ?2",
                    params![existing, id, oar, ofr, oen, o.product_id, o.sort_order, now],
                )?;
                if changed == 0 {
                    return Err(AppError::validation(
                        &format!("options.{i}.id"),
                        "value_invalid",
                    ));
                }
                existing.clone()
            }
            None => {
                let option_id = new_id();
                tx.execute(
                    "INSERT INTO modifier_options (id, group_id, name_ar, name_fr, name_en, product_id, sort_order,
                                                   created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    params![option_id, id, oar, ofr, oen, o.product_id, o.sort_order, now],
                )?;
                option_id
            }
        };
        kept.push(option_id);
    }
    // Options left out of the input are archived (past ticket lines still name them).
    let mut stmt =
        tx.prepare("SELECT id FROM modifier_options WHERE group_id = ?1 AND archived_at IS NULL")?;
    let live = stmt
        .query_map([&id], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for gone in live.into_iter().filter(|o| !kept.contains(o)) {
        tx.execute(
            "UPDATE modifier_options SET archived_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![gone, now],
        )?;
    }
    audit::append(
        tx,
        &actor.event(
            "modifier_group.saved",
            "modifier_group",
            Some(&id),
            json!({ "nameFr": fr, "options": kept.len() }),
        ),
    )?;
    group(tx, &id)
}

pub fn archive_group(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    group(tx, id)?;
    tx.execute(
        "UPDATE modifier_groups SET archived_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event(
            "modifier_group.archived",
            "modifier_group",
            Some(id),
            json!({}),
        ),
    )?;
    Ok(())
}

pub fn set_product_groups(
    tx: &Transaction<'_>,
    actor: &Actor,
    product_id: &str,
    group_ids: &[String],
) -> Result<(), AppError> {
    products::get(tx, product_id)?;
    tx.execute(
        "DELETE FROM product_modifier_groups WHERE product_id = ?1",
        [product_id],
    )?;
    for (i, gid) in group_ids.iter().enumerate() {
        group(tx, gid).map_err(|_| {
            AppError::validation(&format!("groupIds.{i}"), "modifier_group_not_found")
        })?;
        if group_ids[..i].contains(gid) {
            return Err(AppError::validation(
                &format!("groupIds.{i}"),
                "value_invalid",
            ));
        }
        tx.execute(
            "INSERT INTO product_modifier_groups (product_id, group_id, sort_order) VALUES (?1, ?2, ?3)",
            params![product_id, gid, i64::try_from(i).unwrap_or(0)],
        )?;
    }
    audit::append(
        tx,
        &actor.event(
            "product.modifiers_set",
            "product",
            Some(product_id),
            json!({ "groupIds": group_ids }),
        ),
    )?;
    Ok(())
}

fn name_in(o: &ModifierOption, lang: Lang) -> String {
    let [first, second, third] = match lang {
        Lang::Ar => [&o.name_ar, &o.name_fr, &o.name_en],
        Lang::Fr => [&o.name_fr, &o.name_ar, &o.name_en],
        Lang::En => [&o.name_en, &o.name_fr, &o.name_ar],
    };
    [first, second, third]
        .into_iter()
        .find(|n| !n.is_empty())
        .cloned()
        .unwrap_or_default()
}

/// Checks `chosen` against the product's groups (every id known and once, each group's minimum
/// and maximum) and splits it into the line's note and its priced option lines.
pub fn apply(
    conn: &Connection,
    product_id: &str,
    lang: Lang,
    chosen: &[String],
) -> Result<Applied, AppError> {
    let groups = for_product(conn, product_id)?;
    let mut seen = std::collections::BTreeSet::new();
    for id in chosen {
        let known = groups.iter().any(|g| g.options.iter().any(|o| &o.id == id));
        if !known || !seen.insert(id.as_str()) {
            return Err(AppError::validation("modifiers", "modifier_choice_invalid"));
        }
    }
    let mut notes = Vec::new();
    let mut options = Vec::new();
    for (gi, g) in groups.iter().enumerate() {
        let picked: Vec<&ModifierOption> = g
            .options
            .iter()
            .filter(|o| chosen.contains(&o.id))
            .collect();
        let n = u8::try_from(picked.len()).unwrap_or(u8::MAX);
        if n < g.min_select || (g.max_select > 0 && n > g.max_select) {
            return Err(AppError::validation(
                &format!("modifiers.{gi}"),
                "modifier_choice_invalid",
            ));
        }
        for o in picked {
            let name = name_in(o, lang);
            if o.product_id.is_none() {
                notes.push(name.clone());
            }
            options.push(AppliedOption {
                option_id: o.id.clone(),
                product_id: o.product_id.clone(),
                name,
            });
        }
    }
    Ok(Applied {
        note: (!notes.is_empty()).then(|| notes.join(", ")),
        options,
    })
}

/// The restaurant template's groups: sauces (up to two), cooking (one) and a priced cheese
/// supplement (a product of 50 DA at 19 %).
pub fn seed(tx: &Transaction<'_>, actor: &Actor) -> Result<u32, AppError> {
    let any: bool = tx.query_row("SELECT EXISTS (SELECT 1 FROM modifier_groups)", [], |r| {
        r.get(0)
    })?;
    if any {
        return Ok(0);
    }
    let opt = |ar: &str, fr: &str, en: &str, product_id: Option<String>, sort_order: i64| {
        ModifierOptionInput {
            id: None,
            name_ar: ar.into(),
            name_fr: fr.into(),
            name_en: en.into(),
            product_id,
            sort_order,
        }
    };
    save_group(
        tx,
        actor,
        None,
        ModifierGroupInput {
            name_ar: "الصلصات".into(),
            name_fr: "Sauces".into(),
            name_en: "Sauces".into(),
            min_select: 0,
            max_select: 2,
            sort_order: 1,
            options: vec![
                opt("الجزائرية", "Algérienne", "Algerian", None, 1),
                opt("هريسة", "Harissa", "Harissa", None, 2),
                opt("مايونيز", "Mayonnaise", "Mayonnaise", None, 3),
                opt("كاتشب", "Ketchup", "Ketchup", None, 4),
            ],
        },
    )?;
    save_group(
        tx,
        actor,
        None,
        ModifierGroupInput {
            name_ar: "درجة الطهي".into(),
            name_fr: "Cuisson".into(),
            name_en: "Cooking".into(),
            min_select: 0,
            max_select: 1,
            sort_order: 2,
            options: vec![
                opt("نصف مطبوخ", "Saignant", "Rare", None, 1),
                opt("متوسط الطهي", "À point", "Medium", None, 2),
                opt("مطبوخ جيداً", "Bien cuit", "Well done", None, 3),
            ],
        },
    )?;
    let rate = crate::catalog::tax_rates::by_code(tx, "TVA19")?
        .ok_or_else(|| AppError::not_found("tax_rate"))?;
    let mut cheese = crate::catalog::products::ProductInput::simple(
        "Supplément fromage",
        crate::catalog::UNIT_PIECE_ID,
        &rate.id,
        crate::money::Money(5_000),
    );
    cheese.name_ar = "إضافة جبن".into();
    cheese.name_en = "Extra cheese".into();
    cheese.track_stock = false;
    let cheese = products::create(tx, actor, cheese)?;
    save_group(
        tx,
        actor,
        None,
        ModifierGroupInput {
            name_ar: "إضافات".into(),
            name_fr: "Suppléments".into(),
            name_en: "Extras".into(),
            min_select: 0,
            max_select: 0,
            sort_order: 3,
            options: vec![opt("جبن", "Fromage", "Cheese", Some(cheese.id), 1)],
        },
    )?;
    Ok(4)
}
