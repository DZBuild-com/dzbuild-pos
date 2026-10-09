//! Promotions and legal soldes (spec §5.1 `promotions`; PRM-003, 005, 006, 021, 022, 024): percent,
//! amount per unit, "buy X get Y" and bundle prices, scoped to everything, categories (with their
//! sub-categories) or products, dated, prioritised. Soldes, liquidations and déballages keep the
//! price practised before the campaign and the stock put on sale (`promotion_products`, the
//! declaration of PRM-022) and are flagged when outside the wali's dates (setting
//! `promotions.soldes_windows`). "Manual" promotions are the named discounts the cashier applies
//! (PRM-003, ordinary promotions only: a legal campaign is always automatic); automatic ones apply
//! by themselves in `sales::cart`. No prize draw exists or will
//! (PRM-025, décret 06-215).

use crate::audit::{self, Actor};
use crate::catalog::{clean_names, products};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::{Money, Qty, Rate};
use crate::party::clean_day;
use crate::sqlenum::sql_text_enum;
use crate::table::{Cell, ColumnKind, Table};
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Params, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

macro_rules! text_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }

            pub fn parse(s: &str) -> Option<$name> {
                match s { $($text => Some($name::$variant),)+ _ => None }
            }
        }

        sql_text_enum!($name);
    };
}

text_enum!(PromoKind {
    Percent => "percent",
    Amount => "amount",
    BuyXGetY => "buy_x_get_y",
    BundlePrice => "bundle_price",
});
text_enum!(Campaign {
    Promotion => "promotion",
    Solde => "solde",
    Liquidation => "liquidation",
    Deballage => "deballage",
});
text_enum!(TriggerMode {
    Auto => "auto",
    Manual => "manual",
});
text_enum!(DiscountSource {
    Promotion => "promotion",
    Customer => "customer",
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PromoScope {
    All,
    Categories { ids: Vec<String> },
    Products { ids: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Promotion {
    pub id: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub kind: PromoKind,
    pub campaign: Campaign,
    pub is_legal_soldes: bool,
    pub trigger_mode: TriggerMode,
    pub scope: PromoScope,
    /// Percent: basis points. Amount: centimes off each unit. Bundle: the bundle's price.
    pub value: i64,
    pub buy_qty: Option<Qty>,
    pub get_qty: Option<Qty>,
    pub min_qty: Qty,
    pub priority: i32,
    pub starts_on: Option<String>,
    pub ends_on: Option<String>,
    pub active: bool,
    pub archived: bool,
    /// A solde whose dates are not inside a declared window of the wali (PRM-021/024).
    pub outside_soldes_window: bool,
    /// Products whose reference price was kept.
    pub products: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PromotionInput {
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub kind: PromoKind,
    pub campaign: Campaign,
    pub trigger_mode: TriggerMode,
    pub scope: PromoScope,
    pub value: i64,
    pub buy_qty: Option<Qty>,
    pub get_qty: Option<Qty>,
    pub min_qty: Qty,
    pub priority: i32,
    pub starts_on: Option<String>,
    pub ends_on: Option<String>,
    pub active: bool,
}

impl Promotion {
    /// An unsaved promotion (engine tests, previews).
    pub fn preview(input: PromotionInput) -> Promotion {
        Promotion {
            id: String::new(),
            name_ar: input.name_ar,
            name_fr: input.name_fr,
            name_en: input.name_en,
            kind: input.kind,
            campaign: input.campaign,
            is_legal_soldes: input.campaign == Campaign::Solde,
            trigger_mode: input.trigger_mode,
            scope: input.scope,
            value: input.value,
            buy_qty: input.buy_qty,
            get_qty: input.get_qty,
            min_qty: input.min_qty,
            priority: input.priority,
            starts_on: input.starts_on,
            ends_on: input.ends_on,
            active: input.active,
            archived: false,
            outside_soldes_window: false,
            products: 0,
        }
    }

    /// The name in `lang`, else in French, Arabic, English.
    fn name_in(&self, lang: Lang) -> &str {
        fn pick(s: &str) -> Option<&str> {
            (!s.is_empty()).then_some(s)
        }
        let own = match lang {
            Lang::Ar => pick(&self.name_ar),
            Lang::Fr => pick(&self.name_fr),
            Lang::En => pick(&self.name_en),
        };
        own.or_else(|| pick(&self.name_fr))
            .or_else(|| pick(&self.name_ar))
            .unwrap_or(&self.name_en)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SoldesWindow {
    pub from: String,
    pub to: String,
}

/// A promotion ready for the cart: its categories expanded with their descendants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivePromotion {
    pub promotion: Promotion,
    pub categories: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AutoDiscount {
    pub source: DiscountSource,
    pub promotion_id: Option<String>,
    /// The promotion's name in the cart's language (`None` for a customer discount).
    pub label: Option<String>,
    pub amount: Money,
}

/// What the cart may apply by itself: the promotions running today and the customer's standing
/// discount (PRC-003, PRM-005).
pub struct PricingRules<'a> {
    pub promos: &'a [ActivePromotion],
    pub customer_discount: Rate,
}

/// One cart line as the engine sees it.
pub struct LineFacts<'a> {
    pub product_id: &'a str,
    pub category_id: Option<&'a str>,
    pub unit_price: Money,
    pub qty: Qty,
    /// `round(unit_price × qty)`, as M1a's line computation.
    pub gross: Money,
}

const SELECT: &str = "SELECT p.id, p.name_ar, p.name_fr, p.name_en, p.kind, p.campaign,
        p.is_legal_soldes, p.trigger_mode, p.scope, p.value, p.buy_qty, p.get_qty, p.min_qty,
        p.priority, p.starts_on, p.ends_on, p.active, p.archived_at IS NOT NULL,
        (SELECT COUNT(*) FROM promotion_products pp WHERE pp.promotion_id = p.id)
     FROM promotions p";

fn map(r: &Row<'_>) -> rusqlite::Result<(Promotion, String)> {
    Ok((
        Promotion {
            id: r.get(0)?,
            name_ar: r.get(1)?,
            name_fr: r.get(2)?,
            name_en: r.get(3)?,
            kind: r.get(4)?,
            campaign: r.get(5)?,
            is_legal_soldes: r.get(6)?,
            trigger_mode: r.get(7)?,
            scope: PromoScope::All,
            value: r.get(9)?,
            buy_qty: r.get(10)?,
            get_qty: r.get(11)?,
            min_qty: r.get(12)?,
            priority: r.get(13)?,
            starts_on: r.get(14)?,
            ends_on: r.get(15)?,
            active: r.get(16)?,
            archived: r.get(17)?,
            outside_soldes_window: false,
            products: r.get(18)?,
        },
        r.get(8)?,
    ))
}

/// The wali's soldes windows (PRM-021), their days in canonical `YYYY-MM-DD` form so that they
/// compare as text.
pub fn soldes_windows(conn: &Connection) -> Result<Vec<SoldesWindow>, AppError> {
    let windows: Vec<SoldesWindow> = serde_json::from_value(crate::settings::get_value(
        conn,
        "promotions.soldes_windows",
    )?)?;
    windows
        .into_iter()
        .map(|w| {
            Ok(SoldesWindow {
                from: clean_day(Some(&w.from), "from")?.unwrap_or(w.from),
                to: clean_day(Some(&w.to), "to")?.unwrap_or(w.to),
            })
        })
        .collect()
}

fn finish(conn: &Connection, (mut p, scope): (Promotion, String)) -> Result<Promotion, AppError> {
    p.scope = serde_json::from_str(&scope)?;
    if p.campaign == Campaign::Solde {
        let windows = soldes_windows(conn)?;
        p.outside_soldes_window = match (&p.starts_on, &p.ends_on) {
            (Some(from), Some(to)) => !windows.iter().any(|w| w.from <= *from && *to <= w.to),
            _ => true,
        };
    }
    Ok(p)
}

fn select(
    conn: &Connection,
    filter: &str,
    params: impl Params,
) -> Result<Vec<Promotion>, AppError> {
    let mut stmt = conn.prepare_cached(&format!("{SELECT} {filter}"))?;
    let rows = stmt
        .query_map(params, map)?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter().map(|r| finish(conn, r)).collect()
}

pub fn get(conn: &Connection, id: &str) -> Result<Promotion, AppError> {
    let row = conn
        .prepare_cached(&format!("{SELECT} WHERE p.id = ?1"))?
        .query_row([id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("promotion"))?;
    finish(conn, row)
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Promotion>, AppError> {
    select(
        conn,
        "WHERE ?1 OR p.archived_at IS NULL
         ORDER BY p.archived_at IS NOT NULL, p.starts_on DESC, p.name_fr",
        [include_archived],
    )
}

/// Checks `input` and puts its names and days in canonical form; returns the names.
fn validate(
    conn: &Connection,
    input: &mut PromotionInput,
) -> Result<(String, String, String), AppError> {
    let names = clean_names(&input.name_ar, &input.name_fr, &input.name_en)?;
    let invalid = |field: &str| AppError::validation(field, "value_invalid");
    let whole = |q: Option<Qty>| q.is_some_and(|q| q.0 >= 1_000 && q.0 % 1_000 == 0);
    match input.kind {
        PromoKind::Percent if !(1..=10_000).contains(&input.value) => return Err(invalid("value")),
        PromoKind::Amount if input.value <= 0 => return Err(invalid("value")),
        PromoKind::BuyXGetY if !whole(input.buy_qty) => return Err(invalid("buyQty")),
        PromoKind::BuyXGetY if !whole(input.get_qty) => return Err(invalid("getQty")),
        PromoKind::BundlePrice
            if !input
                .buy_qty
                .is_some_and(|q| q.0 >= 2_000 && q.0 % 1_000 == 0) =>
        {
            return Err(invalid("buyQty"));
        }
        PromoKind::BundlePrice if input.value <= 0 => return Err(invalid("value")),
        _ => {}
    }
    // A manual promotion is a named discount (PRM-003); soldes, liquidations and déballages apply
    // automatically so their reference prices are kept and their sales recorded (PRM-021/022).
    if input.trigger_mode == TriggerMode::Manual
        && (input.kind != PromoKind::Percent || input.campaign != Campaign::Promotion)
    {
        return Err(invalid("triggerMode"));
    }
    if !(-100..=100).contains(&input.priority) {
        return Err(invalid("priority"));
    }
    if input.min_qty.is_negative() {
        return Err(AppError::validation("minQty", "qty_invalid"));
    }
    input.starts_on = clean_day(input.starts_on.as_deref(), "startsOn")?;
    input.ends_on = clean_day(input.ends_on.as_deref(), "endsOn")?;
    if let (Some(from), Some(to)) = (&input.starts_on, &input.ends_on)
        && from > to
    {
        return Err(AppError::validation("endsOn", "date_invalid"));
    }
    if input.campaign != Campaign::Promotion
        && (input.starts_on.is_none() || input.ends_on.is_none())
    {
        return Err(AppError::validation("startsOn", "promotion_dates_required"));
    }
    match &input.scope {
        PromoScope::All => {}
        PromoScope::Categories { ids } => {
            if ids.is_empty() {
                return Err(invalid("scope"));
            }
            for id in ids {
                crate::catalog::categories::get(conn, id)
                    .map_err(|_| AppError::validation("scope", "category_not_found"))?;
            }
        }
        PromoScope::Products { ids } => {
            if ids.is_empty() {
                return Err(invalid("scope"));
            }
            for id in ids {
                products::get(conn, id)
                    .map_err(|_| AppError::validation("scope", "product_not_found"))?;
            }
        }
    }
    Ok(names)
}

fn descendants(conn: &Connection, roots: &[String]) -> Result<BTreeSet<String>, AppError> {
    let mut set: BTreeSet<String> = roots.iter().cloned().collect();
    let mut frontier = roots.to_vec();
    let mut stmt = conn.prepare_cached("SELECT id FROM categories WHERE parent_id = ?1")?;
    while let Some(parent) = frontier.pop() {
        let children = stmt
            .query_map([&parent], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for child in children {
            if set.insert(child.clone()) {
                frontier.push(child);
            }
        }
    }
    Ok(set)
}

/// Keeps the price practised before the campaign and the stock put on sale for every product in
/// scope (INSERT OR IGNORE: a reference price, once kept, is evidence and never moves). The stock
/// kept is the level at this moment: the stock put on sale when the campaign already runs; for a
/// campaign created ahead of its first day, the declaration reads the append-only ledger as it
/// stood when that day began instead (the kept row can never be updated).
fn snapshot(tx: &Transaction<'_>, id: &str, scope: &PromoScope) -> Result<(), AppError> {
    let product_ids: Vec<String> = match scope {
        PromoScope::Products { ids } => ids.clone(),
        PromoScope::All => {
            let mut stmt = tx.prepare_cached(
                "SELECT id FROM products WHERE archived_at IS NULL AND kind <> 'variant_parent'",
            )?;
            stmt.query_map([], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?
        }
        PromoScope::Categories { ids } => {
            let mut out = Vec::new();
            let mut stmt = tx.prepare_cached(
                "SELECT id FROM products
                 WHERE category_id = ?1 AND archived_at IS NULL AND kind <> 'variant_parent'",
            )?;
            for category in descendants(tx, ids)? {
                let rows = stmt
                    .query_map([&category], |r| r.get(0))?
                    .collect::<Result<Vec<String>, _>>()?;
                out.extend(rows);
            }
            out
        }
    };
    let now = now_ms();
    for product_id in product_ids {
        let p = products::get(tx, &product_id)?;
        let stock = if p.track_stock {
            crate::stock::on_hand(tx, &product_id)?
        } else {
            Qty::ZERO
        };
        tx.execute(
            "INSERT OR IGNORE INTO promotion_products
                 (promotion_id, product_id, reference_price, stock_at_start, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, product_id, p.sell_price, stock, now],
        )?;
    }
    Ok(())
}

fn write(
    tx: &Transaction<'_>,
    id: &str,
    input: &PromotionInput,
    names: (String, String, String),
    insert: bool,
    actor: &Actor,
) -> Result<(), AppError> {
    let scope = serde_json::to_string(&input.scope)?;
    let now = now_ms();
    if insert {
        tx.execute(
            "INSERT INTO promotions (id, name_ar, name_fr, name_en, kind, campaign, trigger_mode,
                 scope, value, buy_qty, get_qty, min_qty, priority, starts_on, ends_on, active,
                 created_at, updated_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                 ?17, ?17, ?18)",
            params![
                id,
                names.0,
                names.1,
                names.2,
                input.kind,
                input.campaign,
                input.trigger_mode,
                scope,
                input.value,
                input.buy_qty,
                input.get_qty,
                input.min_qty,
                input.priority,
                input.starts_on,
                input.ends_on,
                input.active,
                now,
                actor.user_id
            ],
        )?;
    } else {
        tx.execute(
            "UPDATE promotions SET name_ar = ?2, name_fr = ?3, name_en = ?4, kind = ?5,
                 campaign = ?6, trigger_mode = ?7, scope = ?8, value = ?9, buy_qty = ?10,
                 get_qty = ?11, min_qty = ?12, priority = ?13, starts_on = ?14, ends_on = ?15,
                 active = ?16, updated_at = ?17
             WHERE id = ?1",
            params![
                id,
                names.0,
                names.1,
                names.2,
                input.kind,
                input.campaign,
                input.trigger_mode,
                scope,
                input.value,
                input.buy_qty,
                input.get_qty,
                input.min_qty,
                input.priority,
                input.starts_on,
                input.ends_on,
                input.active,
                now
            ],
        )?;
    }
    if input.trigger_mode == TriggerMode::Auto {
        snapshot(tx, id, &input.scope)?;
    }
    Ok(())
}

pub fn create(
    tx: &Transaction<'_>,
    actor: &Actor,
    mut input: PromotionInput,
) -> Result<Promotion, AppError> {
    let names = validate(tx, &mut input)?;
    let id = new_id();
    write(tx, &id, &input, names, true, actor)?;
    let saved = get(tx, &id)?;
    audit::append(
        tx,
        &actor.event(
            "promotion.created",
            "promotion",
            Some(&id),
            json!({ "after": saved }),
        ),
    )?;
    Ok(saved)
}

/// A legal campaign that has sold (`sale_line_promotions`): its declaration (PRM-022) states the
/// prices it practised.
fn has_sold(conn: &Connection, id: &str) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sale_line_promotions WHERE promotion_id = ?1)",
        [id],
        |r| r.get(0),
    )?)
}

/// Once a solde, liquidation or déballage has sold, what sets its prices is frozen: kind,
/// value, buy/get quantities, campaign and first day (`campaign_has_sales`). A second markdown
/// is a new campaign, declared on its own. Names, the last day, the priority and `active` stay
/// editable (a campaign can be stopped or extended).
fn check_frozen(
    conn: &Connection,
    before: &Promotion,
    input: &PromotionInput,
) -> Result<(), AppError> {
    if before.campaign == Campaign::Promotion || !has_sold(conn, &before.id)? {
        return Ok(());
    }
    let same = before.kind == input.kind
        && before.value == input.value
        && before.buy_qty == input.buy_qty
        && before.get_qty == input.get_qty
        && before.campaign == input.campaign
        && before.starts_on == input.starts_on;
    if same {
        Ok(())
    } else {
        Err(AppError::conflict("campaign_has_sales"))
    }
}

pub fn update(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    mut input: PromotionInput,
) -> Result<Promotion, AppError> {
    let before = get(tx, id)?;
    let names = validate(tx, &mut input)?;
    check_frozen(tx, &before, &input)?;
    write(tx, id, &input, names, false, actor)?;
    let after = get(tx, id)?;
    audit::append(
        tx,
        &actor.event(
            "promotion.updated",
            "promotion",
            Some(id),
            json!({ "before": before, "after": after }),
        ),
    )?;
    Ok(after)
}

pub fn archive(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    get(tx, id)?;
    tx.execute(
        "UPDATE promotions SET archived_at = ?2, active = 0, updated_at = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event("promotion.archived", "promotion", Some(id), json!({})),
    )?;
    Ok(())
}

/// Active, not archived, and `day` inside its dates (open ends run forever).
const RUNNING_ON: &str = "p.active = 1 AND p.archived_at IS NULL
     AND (p.starts_on IS NULL OR p.starts_on <= ?1) AND (p.ends_on IS NULL OR ?1 <= p.ends_on)";

/// The automatic promotions running on `day`, highest priority first.
pub fn active_on(conn: &Connection, day: &str) -> Result<Vec<ActivePromotion>, AppError> {
    let running = select(
        conn,
        &format!(
            "WHERE p.trigger_mode = 'auto' AND {RUNNING_ON} ORDER BY p.priority DESC, p.name_fr"
        ),
        [day],
    )?;
    running
        .into_iter()
        .map(|promotion| {
            let categories = match &promotion.scope {
                PromoScope::Categories { ids } => descendants(conn, ids)?,
                _ => BTreeSet::new(),
            };
            Ok(ActivePromotion {
                promotion,
                categories,
            })
        })
        .collect()
}

/// Named discounts the cashier applies by hand (PRM-003), running on `day`.
pub fn presets(conn: &Connection, day: &str) -> Result<Vec<Promotion>, AppError> {
    select(
        conn,
        &format!("WHERE p.trigger_mode = 'manual' AND {RUNNING_ON} ORDER BY p.name_fr"),
        [day],
    )
}

fn in_scope(a: &ActivePromotion, line: &LineFacts<'_>) -> bool {
    match &a.promotion.scope {
        PromoScope::All => true,
        PromoScope::Products { ids } => ids.iter().any(|id| id.as_str() == line.product_id),
        PromoScope::Categories { .. } => line.category_id.is_some_and(|c| a.categories.contains(c)),
    }
}

/// Whole units in `q` (the free items and bundles count whole pieces).
fn pieces(q: Qty) -> i64 {
    q.0.div_euclid(1_000)
}

/// What `p` saves on `line`, never more than the line's gross (a line total never goes
/// negative); `None` when it saves nothing.
fn saving(p: &Promotion, line: &LineFacts<'_>) -> Option<Money> {
    if line.qty < p.min_qty {
        return None;
    }
    let s = match p.kind {
        PromoKind::Percent => line.gross.mul_div(p.value, 10_000)?,
        PromoKind::Amount => Money(p.value).mul_qty(line.qty)?,
        PromoKind::BuyXGetY => {
            let buy = pieces(p.buy_qty?);
            let get = pieces(p.get_qty?);
            let free = pieces(line.qty).checked_div(buy.checked_add(get)?)? * get;
            line.unit_price.checked_mul_i64(free)?
        }
        PromoKind::BundlePrice => {
            let size = pieces(p.buy_qty?);
            let bundles = pieces(line.qty).checked_div(size)?;
            let normal = line.unit_price.checked_mul_i64(size)?;
            let per_bundle = normal.checked_sub(Money(p.value))?;
            if !per_bundle.is_positive() {
                return None;
            }
            per_bundle.checked_mul_i64(bundles)?
        }
    };
    let s = s.min(line.gross);
    s.is_positive().then_some(s)
}

/// The line's best automatic discount (pure): the highest-priority promotion in scope with a
/// saving, the customer's standing discount competing at priority 0; ties go to the bigger
/// saving.
pub fn best_discount(
    rules: &PricingRules<'_>,
    line: &LineFacts<'_>,
    lang: Lang,
) -> Option<AutoDiscount> {
    let mut best: Option<(i32, AutoDiscount)> = None;
    let mut consider = |priority: i32, candidate: AutoDiscount| {
        let better = match &best {
            None => true,
            Some((p, d)) => priority > *p || (priority == *p && candidate.amount > d.amount),
        };
        if better {
            best = Some((priority, candidate));
        }
    };
    for a in rules.promos.iter().filter(|a| in_scope(a, line)) {
        if let Some(amount) = saving(&a.promotion, line) {
            consider(
                a.promotion.priority,
                AutoDiscount {
                    source: DiscountSource::Promotion,
                    promotion_id: Some(a.promotion.id.clone()),
                    label: Some(a.promotion.name_in(lang).to_owned()),
                    amount,
                },
            );
        }
    }
    if let Some(amount) = line
        .gross
        .apply_rate(rules.customer_discount)
        .filter(|m| m.is_positive())
    {
        consider(
            0,
            AutoDiscount {
                source: DiscountSource::Customer,
                promotion_id: None,
                label: None,
                amount: amount.min(line.gross),
            },
        );
    }
    best.map(|(_, d)| d)
}

/// Records which automatic discount each sold line received (RPT-015, PRM-022).
pub fn record_sale(
    tx: &Transaction<'_>,
    document_id: &str,
    lines: &[(String, AutoDiscount)],
) -> Result<(), AppError> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO sale_line_promotions (line_id, document_id, promotion_id, source, amount)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for (line_id, d) in lines {
        stmt.execute(params![
            line_id,
            document_id,
            d.promotion_id,
            d.source,
            d.amount
        ])?;
    }
    Ok(())
}

/// What a campaign sold of each product, net of the goods brought back: `(quantity, amount
/// paid)`, the amount being each line's gross less the campaign's discount, in proportion to
/// what was not returned.
fn practised(conn: &Connection, id: &str) -> Result<BTreeMap<String, (Qty, Money)>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT l.product_id, l.qty, l.unit_price, s.amount,
                COALESCE((SELECT SUM(-r.qty) FROM document_lines r
                          JOIN documents rd ON rd.id = r.document_id
                          WHERE r.origin_line_id = l.id AND rd.status = 'validated'), 0)
         FROM sale_line_promotions s
         JOIN document_lines l ON l.id = s.line_id
         JOIN documents d ON d.id = s.document_id
         WHERE s.promotion_id = ?1 AND d.status = 'validated' AND l.product_id IS NOT NULL",
    )?;
    let lines = stmt
        .query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Qty>(1)?,
                r.get::<_, Money>(2)?,
                r.get::<_, Money>(3)?,
                r.get::<_, Qty>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let overflow = || AppError::Internal("soldes declaration overflow".into());
    let mut out: BTreeMap<String, (Qty, Money)> = BTreeMap::new();
    for (product_id, qty, unit_price, discount, returned) in lines {
        if qty.0 <= 0 {
            continue;
        }
        let kept = qty.checked_sub(returned).ok_or_else(overflow)?;
        let paid = unit_price
            .mul_qty(qty)
            .and_then(|gross| gross.checked_sub(discount))
            .and_then(|paid| paid.mul_div(kept.0, qty.0))
            .ok_or_else(overflow)?;
        let entry = out.entry(product_id).or_insert((Qty::ZERO, Money::ZERO));
        entry.0 = entry.0.checked_add(kept).ok_or_else(overflow)?;
        entry.1 = entry.1.checked_add(paid).ok_or_else(overflow)?;
    }
    Ok(out)
}

/// The soldes declaration (PRM-022, RPT-054): designation, quantity put on sale, price before,
/// price during, reduction and quantity sold. Once the campaign has sold, the price during and
/// the reduction are the ones practised, read from the recorded sales (net of returns); before,
/// the ones its settings will give. The quantity put on sale is the level when the campaign
/// began (`snapshot`).
pub fn declaration(conn: &Connection, id: &str, lang: Lang) -> Result<Table, AppError> {
    let promo = get(conn, id)?;
    let mut table = Table::new(&[
        ("reference", ColumnKind::Code),
        ("product", ColumnKind::Text),
        ("stockAtStart", ColumnKind::Qty),
        ("referencePrice", ColumnKind::Money),
        ("salePrice", ColumnKind::Money),
        ("reduction", ColumnKind::Percent),
        ("qtySold", ColumnKind::Qty),
    ]);
    // A row kept before the first day began reads the ledger at 00:00 of that day (Algiers).
    let mut stmt = conn.prepare_cached(
        "SELECT pp.product_id, pp.reference_price,
                CASE WHEN pr.starts_on IS NOT NULL
                          AND pp.created_at < unixepoch(pr.starts_on) * 1000 - ?2
                     THEN COALESCE((SELECT SUM(m.qty_delta) FROM stock_movements m
                                    JOIN stock_locations loc ON loc.id = m.location_id
                                    WHERE m.product_id = pp.product_id AND loc.kind = 'storage'
                                      AND m.created_at < unixepoch(pr.starts_on) * 1000 - ?2), 0)
                     ELSE pp.stock_at_start END
         FROM promotion_products pp JOIN promotions pr ON pr.id = pp.promotion_id
         WHERE pp.promotion_id = ?1",
    )?;
    let rows = stmt
        .query_map(params![id, crate::time::ALGIERS_OFFSET_MS], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Money>(1)?,
                r.get::<_, Qty>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let sales = practised(conn, id)?;
    for (product_id, reference, stock) in rows {
        let p = products::get(conn, &product_id)?;
        let (sold, paid) = sales
            .get(&product_id)
            .copied()
            .unwrap_or((Qty::ZERO, Money::ZERO));
        let sale_price = if sold.is_positive() {
            // The average price paid per unit.
            paid.mul_div(Qty::ONE.0, sold.0)
        } else {
            // Nothing sold yet: the price the settings give (buy-X-get-Y and bundles have no
            // single price until they sell).
            match promo.kind {
                PromoKind::Percent => reference.mul_div(10_000 - promo.value, 10_000),
                PromoKind::Amount => reference
                    .checked_sub(Money(promo.value))
                    .map(|s| s.max(Money::ZERO)),
                PromoKind::BuyXGetY | PromoKind::BundlePrice => None,
            }
        };
        let reduction = sale_price
            .filter(|_| reference.is_positive())
            .and_then(|s| reference.checked_sub(s))
            .and_then(|d| d.mul_div(10_000, reference.0))
            .and_then(|m| i32::try_from(m.0).ok())
            .map(Rate);
        table.push(vec![
            Cell::opt_text(p.reference.clone()),
            Cell::text(p.name_in(lang)),
            Cell::qty(stock),
            Cell::money(reference),
            sale_price.map_or(Cell::NULL, Cell::money),
            reduction.map_or(Cell::NULL, Cell::rate),
            Cell::qty(sold),
        ])?;
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::shop;

    fn percent(name: &str, bp: i64, scope: PromoScope) -> PromotionInput {
        PromotionInput {
            name_ar: String::new(),
            name_fr: name.into(),
            name_en: String::new(),
            kind: PromoKind::Percent,
            campaign: Campaign::Promotion,
            trigger_mode: TriggerMode::Auto,
            scope,
            value: bp,
            buy_qty: None,
            get_qty: None,
            min_qty: Qty::ZERO,
            priority: 0,
            starts_on: None,
            ends_on: None,
            active: true,
        }
    }

    fn facts(product: &str, price: i64, qty: i64) -> (String, Money, Qty, Money) {
        (
            product.to_owned(),
            Money(price),
            Qty(qty),
            Money(price * qty / 1_000),
        )
    }

    fn best(
        rules: &PricingRules<'_>,
        f: &(String, Money, Qty, Money),
        category: Option<&str>,
    ) -> Option<AutoDiscount> {
        best_discount(
            rules,
            &LineFacts {
                product_id: &f.0,
                category_id: category,
                unit_price: f.1,
                qty: f.2,
                gross: f.3,
            },
            Lang::Fr,
        )
    }

    fn active(input: PromotionInput) -> ActivePromotion {
        ActivePromotion {
            promotion: Promotion::preview(input),
            categories: BTreeSet::new(),
        }
    }

    #[test]
    fn each_kind_computes_its_saving() {
        let mut two_plus_one = percent(
            "2+1",
            0,
            PromoScope::Products {
                ids: vec!["p".into()],
            },
        );
        two_plus_one.kind = PromoKind::BuyXGetY;
        two_plus_one.buy_qty = Some(Qty(2_000));
        two_plus_one.get_qty = Some(Qty(1_000));
        let promos = [active(two_plus_one)];
        let rules = PricingRules {
            promos: &promos,
            customer_discount: Rate::ZERO,
        };
        // 7 yoghurts at 30 DA: two groups of 3 → 2 free.
        assert_eq!(
            best(&rules, &facts("p", 3_000, 7_000), None).map(|d| d.amount),
            Some(Money(6_000))
        );
        let mut bundle = percent("3 pour 250 DA", 0, PromoScope::All);
        bundle.kind = PromoKind::BundlePrice;
        bundle.buy_qty = Some(Qty(3_000));
        bundle.value = 25_000;
        let promos = [active(bundle)];
        let rules = PricingRules {
            promos: &promos,
            customer_discount: Rate::ZERO,
        };
        // 4 at 100 DA: one bundle of 3 for 250 DA → 50 DA saved.
        assert_eq!(
            best(&rules, &facts("x", 10_000, 4_000), None).map(|d| d.amount),
            Some(Money(5_000))
        );
        let mut per_unit = percent("-20 DA", 0, PromoScope::All);
        per_unit.kind = PromoKind::Amount;
        per_unit.value = 2_000;
        let promos = [active(per_unit)];
        let rules = PricingRules {
            promos: &promos,
            customer_discount: Rate::ZERO,
        };
        assert_eq!(
            best(&rules, &facts("x", 10_000, 1_500), None).map(|d| d.amount),
            Some(Money(3_000)),
            "20 DA × 1.5 kg"
        );
    }

    #[test]
    fn priority_wins_then_the_bigger_saving_and_the_customer_discount_competes() {
        let promos = [
            active(PromotionInput {
                priority: 1,
                ..percent("Rentrée -5 %", 500, PromoScope::All)
            }),
            active(percent("Semaine -15 %", 1_500, PromoScope::All)),
        ];
        let rules = PricingRules {
            promos: &promos,
            customer_discount: Rate(2_000),
        };
        let d = best(&rules, &facts("x", 10_000, 1_000), None).unwrap();
        assert_eq!(
            (d.amount, d.label.as_deref()),
            (Money(500), Some("Rentrée -5 %")),
            "priority 1 beats priority 0"
        );
        let no_promo: [ActivePromotion; 0] = [];
        let rules = PricingRules {
            promos: &no_promo,
            customer_discount: Rate(1_000),
        };
        let d = best(&rules, &facts("x", 10_000, 1_000), None).unwrap();
        assert_eq!(
            (d.source, d.amount),
            (DiscountSource::Customer, Money(1_000))
        );
    }

    #[test]
    fn soldes_need_dates_keep_reference_prices_and_warn_outside_the_wali_window() {
        let shop = shop();
        let jacket = shop.product("Veste hiver", Money(890_000), &shop.tva19);
        shop.stock_in(&jacket.id, 12_000);
        shop.set(
            "promotions.soldes_windows",
            serde_json::json!([{ "from": "2027-01-15", "to": "2027-02-25" }]),
        );
        let mut soldes = percent(
            "Soldes d'hiver",
            3_000,
            PromoScope::Products {
                ids: vec![jacket.id.clone()],
            },
        );
        soldes.campaign = Campaign::Solde;
        let actor = shop.owner.actor();
        let s = soldes.clone();
        assert_eq!(
            shop.db.write(move |tx| create(tx, &actor, s)).unwrap_err(),
            AppError::validation("startsOn", "promotion_dates_required")
        );
        soldes.starts_on = Some("2027-01-10".into());
        soldes.ends_on = Some("2027-02-20".into());
        let actor = shop.owner.actor();
        let saved = shop.db.write(move |tx| create(tx, &actor, soldes)).unwrap();
        assert!(saved.is_legal_soldes);
        assert!(
            saved.outside_soldes_window,
            "starts before the wali's first day"
        );
        assert_eq!(saved.products, 1);
        let table = shop
            .db
            .read(move |c| declaration(c, &saved.id, Lang::Fr))
            .unwrap();
        let row = &table.rows[0];
        assert_eq!(
            row[table.column("referencePrice").unwrap()],
            Cell::money(Money(890_000))
        );
        assert_eq!(
            row[table.column("salePrice").unwrap()],
            Cell::money(Money(623_000))
        );
        assert_eq!(
            row[table.column("stockAtStart").unwrap()],
            Cell::qty(Qty(12_000))
        );
        assert_eq!(
            row[table.column("reduction").unwrap()],
            Cell::rate(Rate(3_000))
        );
        let mut inside = percent(
            "Soldes d'hiver (2)",
            2_000,
            PromoScope::Products {
                ids: vec![jacket.id.clone()],
            },
        );
        inside.campaign = Campaign::Solde;
        inside.starts_on = Some(" 2027-01-15".into());
        inside.ends_on = Some("2027-2-25".into());
        let actor = shop.owner.actor();
        let inside = shop.db.write(move |tx| create(tx, &actor, inside)).unwrap();
        assert!(!inside.outside_soldes_window, "inside the wali's dates");
        assert_eq!(
            (inside.starts_on.as_deref(), inside.ends_on.as_deref()),
            (Some("2027-01-15"), Some("2027-02-25")),
            "days are stored in canonical form"
        );
    }

    /// A solde that has sold is evidence: its prices are frozen (a second markdown is a new
    /// campaign), and its declaration states the prices practised, net of what came back.
    #[test]
    fn a_solde_that_sold_is_frozen_and_declares_the_prices_practised() {
        use crate::payments::PaymentInput;
        use crate::returns::{self, ReturnInput, ReturnLineInput, ReturnReason};
        use crate::sales::cart::{Cart, CartLine};
        use crate::sales::sale::{SaleInput, create_sale};
        let shop = shop();
        let jacket = shop.product("Veste hiver", Money(890_000), &shop.tva19);
        shop.stock_in(&jacket.id, 12_000);
        let mut soldes = percent(
            "Soldes d'été",
            3_000,
            PromoScope::Products {
                ids: vec![jacket.id.clone()],
            },
        );
        soldes.campaign = Campaign::Solde;
        soldes.starts_on = Some("2026-09-01".into());
        soldes.ends_on = Some("2026-10-10".into());
        let input = soldes.clone();
        let actor = shop.owner.actor();
        let saved = shop.db.write(move |tx| create(tx, &actor, input)).unwrap();
        // Before any sale the campaign is still editable.
        let mut deeper = soldes.clone();
        deeper.value = 2_500;
        let (actor, id, again) = (shop.owner.actor(), saved.id.clone(), deeper.clone());
        shop.db
            .write(move |tx| update(tx, &actor, &id, again))
            .unwrap();
        let (actor, id, back) = (shop.owner.actor(), saved.id.clone(), soldes.clone());
        shop.db
            .write(move |tx| update(tx, &actor, &id, back))
            .unwrap();
        shop.open_till(0);
        let cart = Cart::new(Lang::Fr, vec![CartLine::product(&jacket.id, Qty(2_000))]);
        let ctx = shop.till();
        let sale = shop
            .db
            .write(move |tx| {
                create_sale(
                    tx,
                    &ctx,
                    SaleInput {
                        cart,
                        payments: vec![PaymentInput::cash(Money(1_300_000))],
                    },
                )
            })
            .unwrap();
        // A second markdown of the same campaign is refused; its name and last day are not.
        let mut second = soldes.clone();
        second.value = 5_000;
        let (actor, id) = (shop.owner.actor(), saved.id.clone());
        assert_eq!(
            shop.db
                .write(move |tx| update(tx, &actor, &id, second))
                .unwrap_err(),
            AppError::conflict("campaign_has_sales")
        );
        let mut later = soldes.clone();
        later.starts_on = Some("2026-09-02".into());
        let (actor, id) = (shop.owner.actor(), saved.id.clone());
        assert_eq!(
            shop.db
                .write(move |tx| update(tx, &actor, &id, later))
                .unwrap_err(),
            AppError::conflict("campaign_has_sales")
        );
        let mut renamed = soldes.clone();
        renamed.name_fr = "Soldes d'été 2026".into();
        renamed.ends_on = Some("2026-10-15".into());
        let (actor, id) = (shop.owner.actor(), saved.id.clone());
        shop.db
            .write(move |tx| update(tx, &actor, &id, renamed))
            .unwrap();
        // One jacket comes back.
        let input = ReturnInput {
            original_id: Some(sale.document.id.clone()),
            lines: vec![ReturnLineInput {
                origin_line_id: Some(sale.document.lines[0].id.clone()),
                product_id: None,
                qty: Qty(1_000),
                unit_price: None,
                restock: true,
                reason: ReturnReason::Defective,
                note: None,
            }],
            refund_method: crate::payments::PaymentMethod::Cash,
            refund_reference: None,
            lang: Lang::Fr,
            note: None,
        };
        let ctx = shop.owner_till();
        shop.db
            .write(move |tx| returns::create_return(tx, &ctx, input))
            .unwrap();
        let id = saved.id.clone();
        let table = shop
            .db
            .read(move |c| declaration(c, &id, Lang::Fr))
            .unwrap();
        let row = &table.rows[0];
        let cell = |key: &str| row[table.column(key).unwrap()].clone();
        assert_eq!(cell("referencePrice"), Cell::money(Money(890_000)));
        assert_eq!(
            cell("salePrice"),
            Cell::money(Money(623_000)),
            "the price paid"
        );
        assert_eq!(cell("reduction"), Cell::rate(Rate(3_000)));
        assert_eq!(cell("qtySold"), Cell::qty(Qty(1_000)), "net of the return");
        assert_eq!(cell("stockAtStart"), Cell::qty(Qty(12_000)));
    }

    /// A campaign created ahead of its first day declares the stock of that first morning, not
    /// the stock of the day it was typed.
    #[test]
    fn a_campaign_created_ahead_declares_the_stock_of_its_first_day() {
        let shop = shop();
        let shirt = shop.product("Chemise", Money(250_000), &shop.tva19);
        shop.stock_in(&shirt.id, 20_000);
        let mut soldes = percent(
            "Soldes d'hiver",
            2_000,
            PromoScope::Products {
                ids: vec![shirt.id.clone()],
            },
        );
        soldes.campaign = Campaign::Solde;
        soldes.starts_on = Some("2099-01-15".into());
        soldes.ends_on = Some("2099-02-25".into());
        let actor = shop.owner.actor();
        let saved = shop.db.write(move |tx| create(tx, &actor, soldes)).unwrap();
        // Goods received after the campaign was typed, before it starts.
        shop.stock_in(&shirt.id, 5_000);
        let id = saved.id.clone();
        let table = shop
            .db
            .read(move |c| declaration(c, &id, Lang::Fr))
            .unwrap();
        assert_eq!(
            table.rows[0][table.column("stockAtStart").unwrap()],
            Cell::qty(Qty(25_000))
        );
        assert_eq!(
            table.rows[0][table.column("salePrice").unwrap()],
            Cell::money(Money(200_000)),
            "nothing sold yet: the price its settings give"
        );
    }

    #[test]
    fn categories_reach_sub_categories_updates_keep_reference_prices_and_archive_stops() {
        use crate::catalog::categories::{self, CategoryInput};
        use crate::catalog::products::{self as catalog, ProductInput};
        let shop = shop();
        let actor = shop.owner.actor();
        let (dairy, yoghurts) = shop
            .db
            .write(move |tx| {
                let dairy = categories::create(
                    tx,
                    &actor,
                    CategoryInput {
                        name_fr: "Crèmerie".into(),
                        ..CategoryInput::default()
                    },
                )?;
                let yoghurts = categories::create(
                    tx,
                    &actor,
                    CategoryInput {
                        parent_id: Some(dairy.id.clone()),
                        name_fr: "Yaourts".into(),
                        ..CategoryInput::default()
                    },
                )?;
                Ok((dairy.id, yoghurts.id))
            })
            .unwrap();
        let mut input = ProductInput::simple(
            "Yaourt",
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva9,
            Money(3_000),
        );
        input.category_id = Some(yoghurts.clone());
        let yoghurt = shop.product_with(input);
        let actor = shop.owner.actor();
        let scope = PromoScope::Categories { ids: vec![dairy] };
        let promo = shop
            .db
            .write(move |tx| create(tx, &actor, percent("Crèmerie -10 %", 1_000, scope)))
            .unwrap();
        assert_eq!(promo.products, 1, "the sub-category's product is kept");
        let running = shop.db.read(|c| active_on(c, "2026-09-28")).unwrap();
        let rules = PricingRules {
            promos: &running,
            customer_discount: Rate::ZERO,
        };
        let line = facts(&yoghurt.id, 3_000, 2_000);
        assert_eq!(
            best(&rules, &line, Some(yoghurts.as_str())).map(|d| d.amount),
            Some(Money(600))
        );
        assert_eq!(best(&rules, &line, None), None, "outside the scope");
        // A price changed after the start never moves the kept reference price.
        let actor = shop.owner.actor();
        let id = yoghurt.id.clone();
        shop.db
            .write(move |tx| {
                catalog::set_price(
                    tx,
                    &actor,
                    &id,
                    crate::catalog::RETAIL_LEVEL_ID,
                    Money(3_500),
                )
            })
            .unwrap();
        let actor = shop.owner.actor();
        let (id, again) = (
            promo.id.clone(),
            percent(
                "Crèmerie -15 %",
                1_500,
                PromoScope::Products {
                    ids: vec![yoghurt.id.clone()],
                },
            ),
        );
        let updated = shop
            .db
            .write(move |tx| update(tx, &actor, &id, again))
            .unwrap();
        assert_eq!((updated.value, updated.products), (1_500, 1));
        let id = promo.id.clone();
        let table = shop
            .db
            .read(move |c| declaration(c, &id, Lang::Fr))
            .unwrap();
        assert_eq!(
            table.rows[0][table.column("referencePrice").unwrap()],
            Cell::money(Money(3_000))
        );
        let actor = shop.owner.actor();
        let id = promo.id.clone();
        shop.db.write(move |tx| archive(tx, &actor, &id)).unwrap();
        assert!(
            shop.db
                .read(|c| active_on(c, "2026-09-28"))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action LIKE 'promotion.%'"),
            3,
            "created, updated, archived"
        );
    }

    #[test]
    fn only_active_promotions_of_the_day_apply_and_presets_are_manual() {
        let shop = shop();
        let actor = shop.owner.actor();
        let mut later = percent("Ramadan", 1_000, PromoScope::All);
        later.starts_on = Some("2027-02-18".into());
        later.ends_on = Some("2027-03-19".into());
        shop.db.write(move |tx| create(tx, &actor, later)).unwrap();
        let actor = shop.owner.actor();
        let mut staff = percent("Personnel", 1_000, PromoScope::All);
        staff.trigger_mode = TriggerMode::Manual;
        shop.db.write(move |tx| create(tx, &actor, staff)).unwrap();
        assert!(
            shop.db
                .read(|c| active_on(c, "2026-09-28"))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            shop.db.read(|c| active_on(c, "2027-02-20")).unwrap().len(),
            1
        );
        assert_eq!(
            shop.db.read(|c| presets(c, "2026-09-28")).unwrap()[0].name_fr,
            "Personnel"
        );
    }

    #[test]
    fn a_legal_campaign_applies_automatically_never_by_hand() {
        let shop = shop();
        let jacket = shop.product("Veste hiver", Money(890_000), &shop.tva19);
        let legal = |campaign: Campaign| {
            let mut input = percent(
                "Soldes d'hiver",
                3_000,
                PromoScope::Products {
                    ids: vec![jacket.id.clone()],
                },
            );
            input.campaign = campaign;
            input.starts_on = Some("2027-01-15".into());
            input.ends_on = Some("2027-02-25".into());
            input
        };
        for campaign in [Campaign::Solde, Campaign::Liquidation, Campaign::Deballage] {
            let mut manual = legal(campaign);
            manual.trigger_mode = TriggerMode::Manual;
            let actor = shop.owner.actor();
            assert_eq!(
                shop.db
                    .write(move |tx| create(tx, &actor, manual))
                    .unwrap_err(),
                AppError::validation("triggerMode", "value_invalid"),
                "a manual {campaign:?} would keep no reference price (PRM-021)"
            );
        }
        assert_eq!(shop.count("SELECT COUNT(*) FROM promotions"), 0);
        let actor = shop.owner.actor();
        let auto = legal(Campaign::Solde);
        let saved = shop.db.write(move |tx| create(tx, &actor, auto)).unwrap();
        let mut manual = legal(Campaign::Solde);
        manual.trigger_mode = TriggerMode::Manual;
        let actor = shop.owner.actor();
        let id = saved.id.clone();
        assert_eq!(
            shop.db
                .write(move |tx| update(tx, &actor, &id, manual))
                .unwrap_err(),
            AppError::validation("triggerMode", "value_invalid"),
            "an automatic solde cannot be turned into a manual one"
        );
        let id = saved.id.clone();
        assert_eq!(
            shop.db.read(move |c| get(c, &id)).unwrap().trigger_mode,
            TriggerMode::Auto
        );
    }
}
