//! Carcass cutting yield (achat carcasse → découpe, VRT-011): a run takes `input_qty` of the
//! carcass out of stock (`production`), brings each cut in (`production`) at its share of the
//! carcass cost, and records what was lost (bone, fat, trimmings). The cost is split by value
//! (weight × the cut's retail price, the butcher's usual key) or by weight, with the largest
//! remainder rounding, so the cuts' costs add up to the carcass cost to the centime. Each cut's
//! CMUP moves (M1a `apply_cmup`). A template keeps the expected share of each cut. A traced
//! carcass leaves its lots through `stock::lots::take`.

use crate::audit::{self, Actor};
use crate::auth::Permission;
use crate::catalog::{clean_opt, products};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::sqlenum::sql_text_enum;
use crate::stock::{self, MovementReason, NewMovement};
use crate::till::TillContext;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CostMethod {
    Value,
    Weight,
}

impl CostMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            CostMethod::Value => "value",
            CostMethod::Weight => "weight",
        }
    }

    pub fn parse(s: &str) -> Option<CostMethod> {
        [CostMethod::Value, CostMethod::Weight]
            .into_iter()
            .find(|m| m.as_str() == s)
    }
}

sql_text_enum!(CostMethod);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TemplateCut {
    pub product_id: String,
    /// Expected share of the carcass weight, in basis points.
    pub expected_bp: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YieldTemplate {
    pub id: String,
    pub source_product_id: String,
    pub name: String,
    pub cuts: Vec<TemplateCut>,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YieldTemplateInput {
    pub source_product_id: String,
    pub name: String,
    pub cuts: Vec<TemplateCut>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RunCut {
    pub product_id: String,
    pub qty: Qty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RunInput {
    pub template_id: Option<String>,
    pub source_product_id: String,
    /// The default location when absent.
    pub location_id: Option<String>,
    pub input_qty: Qty,
    /// HT cost of the carcass; its CMUP × weight when absent.
    pub input_cost: Option<Money>,
    pub method: CostMethod,
    pub cuts: Vec<RunCut>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CutResult {
    pub product_id: String,
    pub name: String,
    pub qty: Qty,
    /// What the template expected for this weight (0 without a template).
    pub expected_qty: Qty,
    /// The cut's share of the carcass weight (basis points).
    pub yield_bp: i32,
    pub cost: Money,
    /// HT cost per unit (kg) of the cut.
    pub unit_cost: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RunPreview {
    pub input_cost: Money,
    pub cuts: Vec<CutResult>,
    pub output_qty: Qty,
    pub loss_qty: Qty,
    pub loss_bp: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YieldRun {
    pub id: String,
    pub number: String,
    pub template_id: Option<String>,
    pub source_product_id: String,
    pub input_qty: Qty,
    pub input_cost: Money,
    pub method: CostMethod,
    pub output_qty: Qty,
    pub loss_qty: Qty,
    pub business_date: String,
    pub cuts: Vec<CutResult>,
}

/// `total` split in proportion to `weights` (largest remainder: floors first, then one centime
/// to the largest fractional parts, ties to the first), every share ≥ 0, the shares summing to
/// `total` exactly. Equal shares when every weight is 0.
pub fn allocate(total: Money, weights: &[i128]) -> Vec<Money> {
    if weights.is_empty() {
        return Vec::new();
    }
    let sum: i128 = weights.iter().map(|w| (*w).max(0)).sum();
    let (weights, sum): (Vec<i128>, i128) = if sum == 0 {
        (
            vec![1; weights.len()],
            i128::try_from(weights.len()).unwrap_or(1),
        )
    } else {
        (weights.iter().map(|w| (*w).max(0)).collect(), sum)
    };
    let t = i128::from(total.0.max(0));
    let mut shares: Vec<i128> = weights.iter().map(|w| t * w / sum).collect();
    let mut rests: Vec<(i128, usize)> = weights
        .iter()
        .enumerate()
        .map(|(i, w)| ((t * w) % sum, i))
        .collect();
    rests.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut left = t - shares.iter().sum::<i128>();
    for (_, i) in rests {
        if left <= 0 {
            break;
        }
        shares[i] += 1;
        left -= 1;
    }
    shares
        .into_iter()
        .map(|s| Money(i64::try_from(s).unwrap_or(0)))
        .collect()
}

fn template(conn: &Connection, id: &str) -> Result<YieldTemplate, AppError> {
    let (source, name, archived): (String, String, bool) = conn
        .query_row(
            "SELECT source_product_id, name, archived_at IS NOT NULL FROM yield_templates WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("yield_template"))?;
    let mut stmt = conn.prepare_cached(
        "SELECT product_id, expected_bp FROM yield_template_cuts WHERE template_id = ?1 ORDER BY sort_order",
    )?;
    let cuts = stmt
        .query_map([id], |r| {
            Ok(TemplateCut {
                product_id: r.get(0)?,
                expected_bp: r.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(YieldTemplate {
        id: id.to_owned(),
        source_product_id: source,
        name,
        cuts,
        archived,
    })
}

pub fn templates(
    conn: &Connection,
    include_archived: bool,
) -> Result<Vec<YieldTemplate>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id FROM yield_templates WHERE (?1 OR archived_at IS NULL) ORDER BY name",
    )?;
    let ids = stmt
        .query_map([include_archived], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.iter().map(|id| template(conn, id)).collect()
}

fn check_cut_product(
    conn: &Connection,
    field: &str,
    product_id: &str,
    source: &str,
) -> Result<(), AppError> {
    let p = products::get(conn, product_id)
        .map_err(|_| AppError::validation(field, "product_not_found"))?;
    if product_id == source || !p.track_stock {
        return Err(AppError::validation(field, "value_invalid"));
    }
    Ok(())
}

pub fn save_template(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: Option<&str>,
    input: YieldTemplateInput,
) -> Result<YieldTemplate, AppError> {
    packs::require(tx, Module::Yield)?;
    let name = clean_opt(Some(input.name.as_str()), "name", 80)?
        .ok_or_else(|| AppError::validation("name", "required"))?;
    products::get(tx, &input.source_product_id)
        .map_err(|_| AppError::validation("sourceProductId", "product_not_found"))?;
    let mut seen = std::collections::BTreeSet::new();
    let mut total = 0_i64;
    for (i, c) in input.cuts.iter().enumerate() {
        check_cut_product(
            tx,
            &format!("cuts.{i}.productId"),
            &c.product_id,
            &input.source_product_id,
        )?;
        if !seen.insert(c.product_id.as_str()) || !(1..=10_000).contains(&c.expected_bp) {
            return Err(AppError::validation(&format!("cuts.{i}"), "value_invalid"));
        }
        total += i64::from(c.expected_bp);
    }
    if input.cuts.is_empty() || total > 10_000 {
        return Err(AppError::validation("cuts", "yield_shares_invalid"));
    }
    let now = now_ms();
    let id = match id {
        Some(id) => {
            template(tx, id)?;
            tx.execute(
                "UPDATE yield_templates SET source_product_id = ?2, name = ?3, updated_at = ?4 WHERE id = ?1",
                params![id, input.source_product_id, name, now],
            )?;
            tx.execute(
                "DELETE FROM yield_template_cuts WHERE template_id = ?1",
                [id],
            )?;
            id.to_owned()
        }
        None => {
            let id = new_id();
            tx.execute(
                "INSERT INTO yield_templates (id, source_product_id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                params![id, input.source_product_id, name, now],
            )?;
            id
        }
    };
    for (i, c) in input.cuts.iter().enumerate() {
        tx.execute(
            "INSERT INTO yield_template_cuts (template_id, product_id, expected_bp, sort_order) VALUES (?1, ?2, ?3, ?4)",
            params![id, c.product_id, c.expected_bp, i64::try_from(i).unwrap_or(0)],
        )?;
    }
    audit::append(
        tx,
        &actor.event(
            "butcher.yield_template_saved",
            "yield_template",
            Some(&id),
            json!({ "name": name, "cuts": input.cuts.len(), "expectedBp": total }),
        ),
    )?;
    template(tx, &id)
}

pub fn archive_template(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    packs::require(tx, Module::Yield)?;
    template(tx, id)?;
    tx.execute(
        "UPDATE yield_templates SET archived_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event(
            "butcher.yield_template_archived",
            "yield_template",
            Some(id),
            json!({}),
        ),
    )?;
    Ok(())
}

/// The run's figures without writing anything (the screen's live preview), the cuts named in
/// `lang` (fr → ar → en fallback).
pub fn preview(conn: &Connection, input: &RunInput, lang: Lang) -> Result<RunPreview, AppError> {
    let source = products::get(conn, &input.source_product_id)
        .map_err(|_| AppError::validation("sourceProductId", "product_not_found"))?;
    if !input.input_qty.is_positive() {
        return Err(AppError::validation("inputQty", "qty_invalid"));
    }
    if input.cuts.is_empty() {
        return Err(AppError::validation("cuts", "required"));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut output = Qty::ZERO;
    for (i, c) in input.cuts.iter().enumerate() {
        check_cut_product(
            conn,
            &format!("cuts.{i}.productId"),
            &c.product_id,
            &source.id,
        )?;
        if !c.qty.is_positive() || !seen.insert(c.product_id.as_str()) {
            return Err(AppError::validation(
                &format!("cuts.{i}.qty"),
                "qty_invalid",
            ));
        }
        output = output
            .checked_add(c.qty)
            .ok_or_else(|| AppError::validation("cuts", "qty_invalid"))?;
    }
    if output > input.input_qty {
        return Err(AppError::validation("cuts", "yield_output_exceeds_input"));
    }
    let input_cost = match input.input_cost {
        Some(c) if !c.is_negative() => c,
        Some(_) => return Err(AppError::validation("inputCost", "amount_invalid")),
        None => source
            .cost_price
            .mul_qty(input.input_qty)
            .ok_or_else(|| AppError::validation("inputCost", "amount_invalid"))?,
    };
    let template = input
        .template_id
        .as_deref()
        .map(|id| template(conn, id))
        .transpose()?;
    let mut rows: Vec<(products::Product, Qty)> = Vec::with_capacity(input.cuts.len());
    for c in &input.cuts {
        rows.push((products::get(conn, &c.product_id)?, c.qty));
    }
    let weights: Vec<i128> = rows
        .iter()
        .map(|(p, q)| match input.method {
            CostMethod::Weight => i128::from(q.0),
            CostMethod::Value => i128::from(q.0) * i128::from(p.sell_price.0),
        })
        .collect();
    let costs = allocate(input_cost, &weights);
    let bp = |part: i64| {
        i32::try_from(i128::from(part) * 10_000 / i128::from(input.input_qty.0)).unwrap_or(0)
    };
    let cuts = rows
        .into_iter()
        .zip(costs)
        .map(|((p, qty), cost)| {
            let expected_bp = template
                .as_ref()
                .and_then(|t| t.cuts.iter().find(|c| c.product_id == p.id))
                .map_or(0, |c| c.expected_bp);
            CutResult {
                expected_qty: Qty(i64::try_from(
                    i128::from(input.input_qty.0) * i128::from(expected_bp) / 10_000,
                )
                .unwrap_or(0)),
                yield_bp: bp(qty.0),
                unit_cost: cost.mul_div(1_000, qty.0).unwrap_or(Money::ZERO),
                name: p.name_in(lang).to_owned(),
                product_id: p.id,
                qty,
                cost,
            }
        })
        .collect();
    let loss = input.input_qty.checked_sub(output).unwrap_or(Qty::ZERO);
    Ok(RunPreview {
        input_cost,
        cuts,
        output_qty: output,
        loss_qty: loss,
        loss_bp: bp(loss.0),
    })
}

/// Cuts the carcass up; the returned cuts are named in `lang`.
pub fn run(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: RunInput,
    lang: Lang,
) -> Result<YieldRun, AppError> {
    packs::require(tx, Module::Yield)?;
    if !ctx.allows(Permission::StockAdjust) {
        return Err(AppError::Permission {
            permission: Permission::StockAdjust.as_str().to_owned(),
        });
    }
    let preview = preview(tx, &input, lang)?;
    let location = match &input.location_id {
        Some(id) => {
            stock::location(tx, id)
                .map_err(|_| AppError::validation("locationId", "stock_location_not_found"))?
                .id
        }
        None => stock::default_location(tx)?.id,
    };
    if stock::level(tx, &input.source_product_id, &location)? < input.input_qty {
        return Err(AppError::validation("inputQty", "yield_source_short"));
    }
    let id = new_id();
    let number =
        crate::numbering::assign_custom(tx, "yield_run", "DC", ctx.business_date())?.number;
    let actor = ctx.actor();
    let source = products::get(tx, &input.source_product_id)?;
    let out_cost = preview
        .input_cost
        .mul_div(1_000, input.input_qty.0)
        .unwrap_or(source.cost_price);
    // A traced carcass leaves its lots first-expired first-out (one plain row without lots).
    stock::lots::take(
        tx,
        &actor,
        &NewMovement {
            product_id: source.id.clone(),
            location_id: location.clone(),
            qty_delta: input
                .input_qty
                .checked_neg()
                .ok_or_else(|| AppError::validation("inputQty", "qty_invalid"))?,
            reason: MovementReason::Production,
            reason_code: None,
            ref_type: Some("yield_run".into()),
            ref_id: Some(id.clone()),
            unit_cost: Some(out_cost),
            note: None,
        },
        &ctx.business_day(),
        ctx.now,
    )?;
    tx.execute(
        "INSERT INTO yield_runs (id, number, template_id, source_product_id, location_id, input_qty, input_cost, method,
                                 output_qty, loss_qty, business_date, note, created_at, user_id, terminal_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            id,
            number,
            input.template_id,
            source.id,
            location,
            input.input_qty,
            preview.input_cost,
            input.method,
            preview.output_qty,
            preview.loss_qty,
            ctx.business_day(),
            clean_opt(input.note.as_deref(), "note", 200)?,
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    for cut in &preview.cuts {
        let product = products::get(tx, &cut.product_id)?;
        stock::apply_cmup(
            tx,
            &product,
            cut.qty,
            cut.unit_cost,
            "yield_run",
            Some(&id),
            ctx.now,
        )?;
        let movement = stock::append(
            tx,
            &actor,
            &NewMovement {
                product_id: cut.product_id.clone(),
                location_id: location.clone(),
                qty_delta: cut.qty,
                reason: MovementReason::Production,
                reason_code: None,
                ref_type: Some("yield_run".into()),
                ref_id: Some(id.clone()),
                unit_cost: Some(cut.unit_cost),
                note: None,
            },
            ctx.now,
        )?;
        tx.execute(
            "INSERT INTO yield_run_cuts (run_id, product_id, qty, expected_qty, cost, unit_cost, movement_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                cut.product_id,
                cut.qty,
                cut.expected_qty,
                cut.cost,
                cut.unit_cost,
                movement
            ],
        )?;
    }
    audit::append(
        tx,
        &actor.event(
            "butcher.yield_run",
            "yield_run",
            Some(&id),
            json!({
                "number": number,
                "inputQty": input.input_qty,
                "outputQty": preview.output_qty,
                "lossQty": preview.loss_qty,
                "inputCost": preview.input_cost,
                "approvals": ctx.approvers(),
            }),
        ),
    )?;
    Ok(YieldRun {
        id,
        number,
        template_id: input.template_id,
        source_product_id: source.id,
        input_qty: input.input_qty,
        input_cost: preview.input_cost,
        method: input.method,
        output_qty: preview.output_qty,
        loss_qty: preview.loss_qty,
        business_date: ctx.business_day(),
        cuts: preview.cuts,
    })
}

/// The runs of `from..=to`, their cuts named in `lang` (fr → ar → en fallback) and sorted by
/// that name.
pub fn runs(
    conn: &Connection,
    from: &str,
    to: &str,
    lang: Lang,
) -> Result<Vec<YieldRun>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, number, template_id, source_product_id, input_qty, input_cost, method, output_qty, loss_qty, business_date
         FROM yield_runs WHERE business_date BETWEEN ?1 AND ?2 ORDER BY created_at",
    )?;
    let mut runs = stmt
        .query_map(params![from, to], |r| {
            Ok(YieldRun {
                id: r.get(0)?,
                number: r.get(1)?,
                template_id: r.get(2)?,
                source_product_id: r.get(3)?,
                input_qty: r.get(4)?,
                input_cost: r.get(5)?,
                method: r.get(6)?,
                output_qty: r.get(7)?,
                loss_qty: r.get(8)?,
                business_date: r.get(9)?,
                cuts: Vec::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let name = crate::reports::name_of("p", lang);
    let mut cuts = conn.prepare_cached(&format!(
        "SELECT c.product_id, {name} AS name, c.qty, c.expected_qty, c.cost, c.unit_cost FROM yield_run_cuts c
         JOIN products p ON p.id = c.product_id WHERE c.run_id = ?1 ORDER BY name, c.product_id"
    ))?;
    for run in &mut runs {
        let input = run.input_qty.0;
        run.cuts = cuts
            .query_map([&run.id], |r| {
                let qty: Qty = r.get(2)?;
                Ok(CutResult {
                    product_id: r.get(0)?,
                    name: r.get(1)?,
                    qty,
                    expected_qty: r.get(3)?,
                    yield_bp: i32::try_from(i128::from(qty.0) * 10_000 / i128::from(input.max(1)))
                        .unwrap_or(0),
                    cost: r.get(4)?,
                    unit_cost: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }
    Ok(runs)
}
