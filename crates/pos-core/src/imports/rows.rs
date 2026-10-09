//! One row of each import: identify, build the input from the cells (empty cells never erase),
//! call the service that owns the record (it validates and journals like a typed entry), and say
//! what undo needs. A refusal comes back as a `Problem`; only an internal error stops the batch.

use super::engine::{ImportOptions, ImportRow, problem_of};
use super::parse;
use super::spec::{FieldKind, FieldSpec, ImportEntity, ImportPolicy, Problem};
use crate::auth::Permission;
use crate::catalog::categories::{self, CategoryInput};
use crate::catalog::extras::{self, ProductExtrasInput};
use crate::catalog::products::{self, Barcode, LevelPrice, ProductInput};
use crate::catalog::{UNIT_PIECE_ID, normalize::normalize};
use crate::customers::{self, CustomerInput};
use crate::error::AppError;
use crate::ledger::{self, EntryKind, NewEntry, Side};
use crate::money::{Money, Qty, Rate};
use crate::party::western_digits;
use crate::stock::{self, AdjustInput, AdjustReason, counts};
use crate::suppliers::{self, SupplierInput};
use crate::table::TagDomain;
use crate::till::TillContext;
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) struct Env<'a> {
    pub ctx: &'a TillContext,
    pub opts: &'a ImportOptions,
    pub choices: &'a BTreeMap<String, String>,
    /// The fields of `opts.entity` (`spec::fields`).
    pub fields: &'a [FieldSpec],
}

pub(crate) enum Outcome {
    Created(String),
    Updated(String, Value),
    /// A ledger or stock row, with what its counter-entry needs.
    Posted(String, Value),
    /// Not applied (a problem code), with the existing record it matched.
    Skipped(&'static str, Option<String>),
}

type RowResult = Result<Result<Outcome, Problem>, AppError>;

/// A service refusal becomes the row's problem; an internal error stops the batch.
fn refused(row: &ImportRow, field: Option<&str>, err: AppError) -> RowResult {
    match err {
        AppError::Validation { .. }
        | AppError::Conflict { .. }
        | AppError::NotFound { .. }
        | AppError::Rejected { .. } => Ok(Err(problem_of(row, field, &err))),
        other => Err(other),
    }
}

/// `try` for cell parsing (a problem ends the row) and for services (see `refused`).
macro_rules! cell {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(p) => return Ok(Err(p)),
        }
    };
}
macro_rules! service {
    ($row:expr, $field:expr, $e:expr) => {
        match $e {
            Ok(v) => v,
            Err(err) => return refused($row, $field, err),
        }
    };
}

struct Cells<'a>(&'a ImportRow);

impl<'a> Cells<'a> {
    fn text(&self, key: &str) -> Option<&'a str> {
        self.0.cells.get(key).map(String::as_str)
    }

    fn owned(&self, key: &str) -> Option<String> {
        self.text(key).map(str::to_owned)
    }

    fn problem(&self, key: &str, code: &str) -> Problem {
        Problem {
            row: self.0.number,
            field: Some(key.to_owned()),
            value: self.owned(key),
            code: code.to_owned(),
        }
    }

    fn money(&self, key: &str, signed: bool) -> Result<Option<Money>, Problem> {
        let Some(text) = self.text(key) else {
            return Ok(None);
        };
        match parse::money(text) {
            Ok(Some(m)) if !signed && m.is_negative() => Err(self.problem(key, "negative")),
            Ok(v) => Ok(v),
            Err(code) => Err(self.problem(key, code)),
        }
    }

    fn qty(&self, key: &str, decimals: u8) -> Result<Option<Qty>, Problem> {
        let Some(text) = self.text(key) else {
            return Ok(None);
        };
        match parse::qty(text, decimals) {
            Ok(Some(q)) if q.is_negative() => Err(self.problem(key, "negative")),
            Ok(v) => Ok(v),
            Err(code) => Err(self.problem(key, code)),
        }
    }

    fn percent(&self, key: &str) -> Result<Option<i64>, Problem> {
        self.text(key).map_or(Ok(None), |t| {
            parse::percent_bp(t).map_err(|c| self.problem(key, c))
        })
    }

    fn int(&self, key: &str) -> Result<Option<i64>, Problem> {
        self.text(key).map_or(Ok(None), |t| {
            parse::int(t).map_err(|c| self.problem(key, c))
        })
    }

    fn boolean(&self, key: &str) -> Result<Option<bool>, Problem> {
        self.text(key).map_or(Ok(None), |t| {
            parse::boolean(t).map_err(|c| self.problem(key, c))
        })
    }

    fn choice(
        &self,
        key: &str,
        domain: TagDomain,
        known: &BTreeMap<String, String>,
    ) -> Result<Option<String>, Problem> {
        self.text(key).map_or(Ok(None), |t| {
            parse::choice(t, domain, known).map_err(|c| self.problem(key, c))
        })
    }

    fn code(&self, key: &str) -> Option<String> {
        self.text(key).map(|t| western_digits(t).trim().to_owned())
    }
}

/// A spreadsheet error value (`#N/A` of a failed VLOOKUP, …) in a text, code or list cell refuses
/// the row instead of being stored as a name or a code. Number, date and yes/no cells already
/// refuse it through their parsers.
fn excel_error(env: &Env<'_>, row: &ImportRow) -> Option<Problem> {
    row.cells.iter().find_map(|(key, value)| {
        let textual = env.fields.iter().any(|f| {
            &f.key == key && matches!(f.kind, FieldKind::Text | FieldKind::Code | FieldKind::List)
        });
        (textual && parse::excel_error(value)).then(|| Problem {
            row: row.number,
            field: Some(key.clone()),
            value: Some(value.clone()),
            code: "value_invalid".to_owned(),
        })
    })
}

fn id_by(tx: &Transaction<'_>, sql: &str, value: &str) -> Result<Option<String>, AppError> {
    Ok(tx.query_row(sql, [value], |r| r.get(0)).optional()?)
}

/// A product by its reference, then by any of its barcodes.
fn product_id(tx: &Transaction<'_>, c: &Cells<'_>) -> Result<Option<String>, AppError> {
    if let Some(reference) = c.code("reference")
        && let Some(id) = id_by(
            tx,
            "SELECT id FROM products WHERE reference = ?1",
            &reference,
        )?
    {
        return Ok(Some(id));
    }
    for code in c.text("barcode").map(parse::list).unwrap_or_default() {
        if let Some(id) = id_by(
            tx,
            "SELECT product_id FROM product_barcodes WHERE barcode = ?1",
            &code,
        )? {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

/// The existing product a products row names, found the way the row finds it, whatever then
/// becomes of the row (refused, repeated): a sync never archives a product that is in the file.
pub(crate) fn named_product(
    tx: &Transaction<'_>,
    row: &ImportRow,
) -> Result<Option<String>, AppError> {
    product_id(tx, &Cells(row))
}

fn is_arabic(text: &str) -> bool {
    text.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c))
}

/// A category by its name in any language; created (journaled) when absent.
fn category_id(
    tx: &Transaction<'_>,
    env: &Env<'_>,
    name: &str,
    parent_id: Option<String>,
) -> Result<String, AppError> {
    let wanted = normalize(name);
    for c in categories::list(tx, false)? {
        if [&c.name_fr, &c.name_ar, &c.name_en]
            .iter()
            .any(|n| !n.is_empty() && normalize(n) == wanted)
        {
            return Ok(c.id);
        }
    }
    let mut input = CategoryInput {
        parent_id,
        ..CategoryInput::default()
    };
    if is_arabic(name) {
        input.name_ar = name.trim().to_owned();
    } else {
        input.name_fr = name.trim().to_owned();
    }
    Ok(categories::create(tx, &env.ctx.actor(), input)?.id)
}

fn product(tx: &Transaction<'_>, env: &Env<'_>, row: &ImportRow) -> RowResult {
    let c = Cells(row);
    let found = product_id(tx, &c)?;
    if found.is_some() && env.opts.policy == ImportPolicy::CreateOnly {
        return Ok(Ok(Outcome::Skipped("already_exists", found)));
    }
    let current = match &found {
        Some(id) => Some(products::get(tx, id)?),
        None => None,
    };
    let mut input = match &current {
        Some(p) => ProductInput::from(p),
        None => {
            let standard: Option<String> = tx
                .query_row(
                    "SELECT id FROM tax_rates WHERE active = 1 ORDER BY rate_bp DESC, code LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            ProductInput::simple(
                "",
                UNIT_PIECE_ID,
                &standard.unwrap_or_default(),
                Money::ZERO,
            )
        }
    };
    let before_extras = match &found {
        Some(id) => ProductExtrasInput::from(&extras::get(tx, id)?),
        None => ProductExtrasInput::default(),
    };
    let before_input = input.clone();
    if let Some(r) = c.code("reference") {
        input.reference = Some(r);
    }
    for (key, slot) in [
        ("nameFr", &mut input.name_fr),
        ("nameAr", &mut input.name_ar),
        ("nameEn", &mut input.name_en),
    ] {
        if let Some(v) = c.owned(key) {
            *slot = v;
        }
    }
    if input.name_fr.trim().is_empty()
        && input.name_ar.trim().is_empty()
        && input.name_en.trim().is_empty()
    {
        return Ok(Err(c.problem("nameFr", "required")));
    }
    let decimals: u8 = match c.code("unit") {
        Some(unit) => {
            let found: Option<(String, u8)> = tx
                .query_row(
                    "SELECT id, decimals FROM units WHERE code = ?1 COLLATE NOCASE AND archived_at IS NULL",
                    [&unit],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let Some((id, d)) = found else {
                return Ok(Err(c.problem("unit", "unit_not_found")));
            };
            input.unit_id = id;
            d
        }
        None => tx.query_row(
            "SELECT decimals FROM units WHERE id = ?1",
            [&input.unit_id],
            |r| r.get(0),
        )?,
    };
    if let Some(bp) = cell!(c.percent("taxRate")) {
        let rate: Option<String> = tx
            .query_row(
                "SELECT id FROM tax_rates WHERE rate_bp = ?1 AND active = 1 ORDER BY code LIMIT 1",
                [bp],
                |r| r.get(0),
            )
            .optional()?;
        let Some(rate) = rate else {
            return Ok(Err(c.problem("taxRate", "tax_rate_not_found")));
        };
        input.tax_rate_id = rate;
    }
    match cell!(c.money("sellPrice", false)) {
        Some(price) => input.sell_price = price,
        None if current.is_none() => return Ok(Err(c.problem("sellPrice", "required"))),
        None => {}
    }
    if let Some(cost) = cell!(c.money("costPrice", false)) {
        input.cost_price = cost;
    }
    if let Some(min) = cell!(c.qty("minStock", decimals)) {
        input.min_stock = min;
    }
    if let Some(track) = cell!(c.boolean("trackStock")) {
        input.track_stock = track;
    }
    if let Some(name) = c.text("category") {
        input.category_id = Some(service!(
            row,
            Some("category"),
            category_id(tx, env, name, None)
        ));
    }
    for code in c.text("barcode").map(parse::list).unwrap_or_default() {
        if !input.barcodes.iter().any(|b| b.barcode == code) {
            input.barcodes.push(Barcode {
                barcode: code,
                pack_qty: Qty(1_000),
                label: None,
            });
        }
    }
    for key in row.cells.keys() {
        let Some(level_code) = key.strip_prefix("price.") else {
            continue;
        };
        let Some(price) = cell!(c.money(key, false)) else {
            continue;
        };
        let level: String = tx.query_row(
            "SELECT id FROM price_levels WHERE code = ?1",
            [level_code],
            |r| r.get(0),
        )?;
        input.prices.retain(|p| p.price_level_id != level);
        input.prices.push(LevelPrice {
            price_level_id: level,
            price,
        });
    }
    // Like `products_update`: an existing product keeps its prices without `products.price`
    // and its CMUP without `reports.profit`, whatever the file says (`engine` refuses such a
    // mapping first; this is the second line).
    if let Some(p) = &current {
        if !env.ctx.allows(Permission::ProductsPrice) {
            input.sell_price = p.sell_price;
            input.prices.clone_from(&p.prices);
        }
        if !env.ctx.allows(Permission::ReportsProfit) {
            input.cost_price = p.cost_price;
        }
    }
    let actor = env.ctx.actor();
    let saved = match &current {
        Some(p) => service!(row, None, products::update(tx, &actor, &p.id, input)),
        None => service!(row, None, products::create(tx, &actor, input)),
    };
    let mut extras_input = before_extras.clone();
    let mut extras_changed = false;
    for (key, slot) in [
        ("brand", &mut extras_input.brand),
        ("supplierRef", &mut extras_input.supplier_ref),
    ] {
        if let Some(v) = c.owned(key) {
            *slot = Some(v);
            extras_changed = true;
        }
    }
    if extras_changed {
        // `set_quiet` refreshes the searchable text too; the product row's audit covers it.
        service!(row, None, extras::set_quiet(tx, &saved.id, extras_input));
    }
    // Undo restores only what the row set, and the CMUP only while nothing moved it since.
    let set: Vec<&String> = row.cells.keys().collect();
    Ok(Ok(match current {
        Some(_) => Outcome::Updated(
            saved.id,
            json!({ "product": before_input, "extras": before_extras, "fields": set,
                    "costAfter": saved.cost_price }),
        ),
        None => Outcome::Created(saved.id),
    }))
}

fn category(tx: &Transaction<'_>, env: &Env<'_>, row: &ImportRow) -> RowResult {
    let c = Cells(row);
    let names: Vec<String> = ["nameFr", "nameAr", "nameEn"]
        .iter()
        .filter_map(|k| c.owned(k))
        .collect();
    if names.is_empty() {
        return Ok(Err(c.problem("nameFr", "required")));
    }
    let wanted: Vec<String> = names.iter().map(|n| normalize(n)).collect();
    let existing = categories::list(tx, false)?.into_iter().find(|cat| {
        [&cat.name_fr, &cat.name_ar, &cat.name_en]
            .iter()
            .any(|n| !n.is_empty() && wanted.contains(&normalize(n)))
    });
    if existing.is_some() && env.opts.policy == ImportPolicy::CreateOnly {
        return Ok(Ok(Outcome::Skipped("already_exists", None)));
    }
    let parent = match c.text("parent") {
        Some(p) => Some(service!(row, Some("parent"), category_id(tx, env, p, None))),
        None => None,
    };
    let mut input = existing
        .as_ref()
        .map(CategoryInput::from)
        .unwrap_or_default();
    let before = input.clone();
    for (key, slot) in [
        ("nameFr", &mut input.name_fr),
        ("nameAr", &mut input.name_ar),
        ("nameEn", &mut input.name_en),
    ] {
        if let Some(v) = c.owned(key) {
            *slot = v;
        }
    }
    if parent.is_some() {
        input.parent_id = parent;
    }
    let actor = env.ctx.actor();
    Ok(Ok(match existing {
        Some(cat) => {
            service!(row, None, categories::update(tx, &actor, &cat.id, input));
            Outcome::Updated(cat.id, serde_json::to_value(before)?)
        }
        None => Outcome::Created(service!(row, None, categories::create(tx, &actor, input)).id),
    }))
}

fn party_text(c: &Cells<'_>, fields: &mut [(&str, &mut Option<String>)]) {
    for (key, slot) in fields.iter_mut() {
        if let Some(v) = c.owned(key) {
            **slot = Some(v);
        }
    }
}

fn customer(tx: &Transaction<'_>, env: &Env<'_>, row: &ImportRow) -> RowResult {
    let c = Cells(row);
    let mut found = match c.code("code") {
        Some(code) => id_by(
            tx,
            "SELECT id FROM customers WHERE code = ?1 COLLATE NOCASE",
            &code,
        )?,
        None => None,
    };
    if found.is_none()
        && let Some(phone) = c.text("phone")
    {
        found = service!(row, Some("phone"), customers::find_by_phone(tx, phone)).map(|x| x.id);
    }
    if found.is_some() && env.opts.policy == ImportPolicy::CreateOnly {
        return Ok(Ok(Outcome::Skipped("already_exists", None)));
    }
    let current = match &found {
        Some(id) => Some(customers::get(tx, id)?),
        None => None,
    };
    let mut input = current
        .as_ref()
        .map(CustomerInput::from)
        .unwrap_or_default();
    let before = input.clone();
    if let Some(code) = c.code("code") {
        input.code = Some(code);
    }
    if let Some(name) = c.owned("name") {
        input.name = name;
    }
    if let Some(kind) = cell!(c.choice("customerType", TagDomain::CustomerType, env.choices)) {
        input.customer_type = cell!(
            customers::CustomerType::parse(&kind)
                .ok_or_else(|| c.problem("customerType", "choice_invalid"))
        );
    }
    party_text(
        &c,
        &mut [
            ("phone", &mut input.phone),
            ("phone2", &mut input.phone2),
            ("email", &mut input.email),
            ("address", &mut input.address),
            ("wilaya", &mut input.wilaya),
            ("nif", &mut input.nif),
            ("nis", &mut input.nis),
            ("rc", &mut input.rc),
            ("ai", &mut input.ai),
            ("notes", &mut input.notes),
        ],
    );
    if let Some(limit) = cell!(c.money("creditLimit", false)) {
        input.credit_limit = Some(limit);
    }
    if let Some(bp) = cell!(c.percent("discount")) {
        input.discount_bp = Rate(i32::try_from(bp).unwrap_or(0));
    }
    // Like `customers_update`: a limit is kridi data (`credit.view`).
    if let Some(existing) = &current
        && !env.ctx.allows(Permission::CreditView)
    {
        input.credit_limit = existing.credit_limit;
    }
    let actor = env.ctx.actor();
    Ok(Ok(match current {
        Some(existing) => {
            service!(
                row,
                None,
                customers::update(tx, &actor, &env.ctx.pack, &existing.id, input)
            );
            Outcome::Updated(existing.id, serde_json::to_value(before)?)
        }
        None => Outcome::Created(
            service!(
                row,
                None,
                customers::create(tx, &actor, &env.ctx.pack, input)
            )
            .id,
        ),
    }))
}

fn supplier(tx: &Transaction<'_>, env: &Env<'_>, row: &ImportRow) -> RowResult {
    let c = Cells(row);
    let found = match c.code("code") {
        Some(code) => id_by(
            tx,
            "SELECT id FROM suppliers WHERE code = ?1 COLLATE NOCASE",
            &code,
        )?,
        None => None,
    };
    if found.is_some() && env.opts.policy == ImportPolicy::CreateOnly {
        return Ok(Ok(Outcome::Skipped("already_exists", None)));
    }
    let current = match &found {
        Some(id) => Some(suppliers::get(tx, id)?),
        None => None,
    };
    let mut input = current
        .as_ref()
        .map(SupplierInput::from)
        .unwrap_or_default();
    let before = input.clone();
    if let Some(code) = c.code("code") {
        input.code = Some(code);
    }
    if let Some(name) = c.owned("name") {
        input.name = name;
    }
    party_text(
        &c,
        &mut [
            ("phone", &mut input.phone),
            ("email", &mut input.email),
            ("address", &mut input.address),
            ("wilaya", &mut input.wilaya),
            ("nif", &mut input.nif),
            ("nis", &mut input.nis),
            ("rc", &mut input.rc),
            ("ai", &mut input.ai),
            ("rib", &mut input.rib),
            ("notes", &mut input.notes),
        ],
    );
    if let Some(days) = cell!(c.int("paymentTermsDays")) {
        input.payment_terms_days = Some(days);
    }
    let actor = env.ctx.actor();
    Ok(Ok(match current {
        Some(existing) => {
            service!(
                row,
                None,
                suppliers::update(tx, &actor, &env.ctx.pack, &existing.id, input)
            );
            Outcome::Updated(existing.id, serde_json::to_value(before)?)
        }
        None => Outcome::Created(
            service!(
                row,
                None,
                suppliers::create(tx, &actor, &env.ctx.pack, input)
            )
            .id,
        ),
    }))
}

fn unit_decimals(tx: &Transaction<'_>, product_id: &str) -> Result<u8, AppError> {
    Ok(tx.query_row(
        "SELECT u.decimals FROM products p JOIN units u ON u.id = p.unit_id WHERE p.id = ?1",
        [product_id],
        |r| r.get(0),
    )?)
}

fn opening_stock(tx: &Transaction<'_>, env: &Env<'_>, row: &ImportRow) -> RowResult {
    let c = Cells(row);
    let Some(id) = product_id(tx, &c)? else {
        return Ok(Err(c.problem("reference", "not_found")));
    };
    let decimals = unit_decimals(tx, &id)?;
    let Some(qty) = cell!(c.qty("qty", decimals)) else {
        return Ok(Err(c.problem("qty", "required")));
    };
    if qty.0 <= 0 {
        return Ok(Err(c.problem("qty", "value_invalid")));
    }
    let unit_cost = cell!(c.money("unitCost", false));
    let location_id = match c.code("location") {
        Some(code) => match id_by(
            tx,
            "SELECT id FROM stock_locations WHERE code = ?1 COLLATE NOCASE AND archived_at IS NULL",
            &code,
        )? {
            Some(l) => l,
            None => return Ok(Err(c.problem("location", "location_not_found"))),
        },
        None => match &env.opts.location_id {
            Some(l) => l.clone(),
            None => stock::default_location(tx)?.id,
        },
    };
    let input = AdjustInput {
        product_id: id.clone(),
        location_id: Some(location_id.clone()),
        qty_delta: qty,
        reason: AdjustReason::Opening,
        unit_cost,
        note: Some(env.opts.file_name.clone()),
    };
    let cost_before = products::get(tx, &id)?.cost_price;
    let moved = service!(
        row,
        None,
        stock::adjust(tx, &env.ctx.actor(), input, env.ctx.now)
    );
    let cost_after = products::get(tx, &id)?.cost_price;
    Ok(Ok(Outcome::Posted(
        moved.id,
        json!({ "posting": "stock", "productId": id, "locationId": location_id, "qty": qty,
                "costBefore": cost_before, "costAfter": cost_after }),
    )))
}

fn opening_balance(tx: &Transaction<'_>, env: &Env<'_>, row: &ImportRow, side: Side) -> RowResult {
    let c = Cells(row);
    let party = match side {
        Side::Customer => {
            let mut id = match c.code("code") {
                Some(code) => id_by(
                    tx,
                    "SELECT id FROM customers WHERE code = ?1 COLLATE NOCASE",
                    &code,
                )?,
                None => None,
            };
            if id.is_none()
                && let Some(phone) = c.text("phone")
            {
                id =
                    service!(row, Some("phone"), customers::find_by_phone(tx, phone)).map(|x| x.id);
            }
            id
        }
        Side::Supplier => match c.code("code") {
            Some(code) => id_by(
                tx,
                "SELECT id FROM suppliers WHERE code = ?1 COLLATE NOCASE",
                &code,
            )?,
            None => None,
        },
    };
    let Some(party) = party else {
        return Ok(Err(c.problem("code", "not_found")));
    };
    let Some(amount) = cell!(c.money("amount", true)) else {
        return Ok(Err(c.problem("amount", "required")));
    };
    let note = c.text("note");
    let entry = match side {
        Side::Customer => service!(
            row,
            Some("amount"),
            customers::credit::set_opening(tx, env.ctx, &party, amount, note)
        ),
        Side::Supplier => service!(
            row,
            Some("amount"),
            suppliers::set_opening(tx, env.ctx, &party, amount, note)
        ),
    };
    let posting = match side {
        Side::Customer => "customer_opening",
        Side::Supplier => "supplier_opening",
    };
    Ok(Ok(Outcome::Posted(
        entry.id.clone(),
        json!({ "posting": posting, "partyId": party, "entryId": entry.id, "amount": amount }),
    )))
}

fn count_line(tx: &Transaction<'_>, env: &Env<'_>, row: &ImportRow) -> RowResult {
    let c = Cells(row);
    let Some(count_id) = env.opts.count_id.clone() else {
        return Ok(Err(c.problem("counted", "rejected")));
    };
    let Some(id) = product_id(tx, &c)? else {
        return Ok(Err(c.problem("reference", "not_found")));
    };
    let decimals = unit_decimals(tx, &id)?;
    let Some(counted) = cell!(c.qty("counted", decimals)) else {
        return Ok(Err(c.problem("counted", "required")));
    };
    let before: Option<(Option<i64>, i64, Option<i64>)> = tx
        .query_row(
            "SELECT counted_qty, expected_qty, counted_at FROM stock_count_lines
             WHERE count_id = ?1 AND product_id = ?2",
            params![count_id, id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    service!(
        row,
        Some("counted"),
        counts::set_counted(tx, env.ctx, &count_id, &id, counted, counts::CountMode::Set)
    );
    let (counted_before, expected_before, counted_at_before) = match before {
        Some((counted, expected, at)) => (counted, Some(expected), at),
        None => (None, None, None),
    };
    Ok(Ok(Outcome::Posted(
        id.clone(),
        json!({ "posting": "count", "countId": count_id, "productId": id,
                "before": counted_before, "expectedBefore": expected_before,
                "countedAtBefore": counted_at_before }),
    )))
}

pub(crate) fn apply(tx: &Transaction<'_>, env: &Env<'_>, row: &ImportRow) -> RowResult {
    if let Some(problem) = excel_error(env, row) {
        return Ok(Err(problem));
    }
    match env.opts.entity {
        ImportEntity::Products => product(tx, env, row),
        ImportEntity::Categories => category(tx, env, row),
        ImportEntity::Customers => customer(tx, env, row),
        ImportEntity::Suppliers => supplier(tx, env, row),
        ImportEntity::OpeningStock => opening_stock(tx, env, row),
        ImportEntity::CustomerBalances => opening_balance(tx, env, row, Side::Customer),
        ImportEntity::SupplierBalances => opening_balance(tx, env, row, Side::Supplier),
        ImportEntity::CountLines => count_line(tx, env, row),
    }
}

fn field<T: serde::de::DeserializeOwned>(value: &Value, key: &str) -> Result<T, AppError> {
    serde_json::from_value(value.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|e| AppError::Internal(format!("import undo record {key}: {e}")))
}

/// Puts back the CMUP an import set (`imported`) to the one before it (`previous`), with its
/// `cost_history` row, only while nothing moved it since: a goods receipt that averaged a new
/// cost in after the import refuses the undo (`import_undo_not_allowed`) rather than erase it.
fn restore_cost(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    job_id: &str,
    product_id: &str,
    imported: Money,
    previous: Money,
) -> Result<(), AppError> {
    let current: Money = tx.query_row(
        "SELECT cost_price FROM products WHERE id = ?1",
        [product_id],
        |r| r.get(0),
    )?;
    if current != imported {
        return Err(AppError::conflict("import_undo_not_allowed"));
    }
    if imported == previous {
        return Ok(());
    }
    tx.execute(
        "UPDATE products SET cost_price = ?2, updated_at = ?3 WHERE id = ?1",
        params![product_id, previous, ctx.now],
    )?;
    tx.execute(
        "INSERT INTO cost_history (id, product_id, cost_before, cost_after, source_type, source_id, created_at)
         VALUES (?1, ?2, ?3, ?4, 'import_undo', ?5, ?6)",
        params![crate::ids::new_id(), product_id, current, previous, job_id, ctx.now],
    )?;
    Ok(())
}

/// Puts back what one products row set, and only that: a price, a name or a barcode changed
/// since by hand on a field the file did not carry stays. Records written before the fields
/// were kept restore every field but the CMUP.
fn undo_product(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    job_id: &str,
    id: &str,
    saved: &Value,
) -> Result<(), AppError> {
    let before: ProductInput = field(saved, "product")?;
    let fields: Option<Vec<String>> = field(saved, "fields")?;
    let set = |key: &str| {
        fields
            .as_ref()
            .is_none_or(|f| f.iter().any(|k| k.as_str() == key))
    };
    let mut input = ProductInput::from(&products::get(tx, id)?);
    if set("reference") {
        input.reference.clone_from(&before.reference);
    }
    for (key, slot, was) in [
        ("nameFr", &mut input.name_fr, &before.name_fr),
        ("nameAr", &mut input.name_ar, &before.name_ar),
        ("nameEn", &mut input.name_en, &before.name_en),
        ("unit", &mut input.unit_id, &before.unit_id),
        ("taxRate", &mut input.tax_rate_id, &before.tax_rate_id),
    ] {
        if set(key) {
            slot.clone_from(was);
        }
    }
    if set("category") {
        input.category_id.clone_from(&before.category_id);
    }
    if set("sellPrice") {
        input.sell_price = before.sell_price;
    }
    if set("minStock") {
        input.min_stock = before.min_stock;
    }
    if set("trackStock") {
        input.track_stock = before.track_stock;
    }
    if set("barcode") {
        input.barcodes.clone_from(&before.barcodes);
    }
    match &fields {
        None => input.prices.clone_from(&before.prices),
        Some(keys) => {
            for code in keys.iter().filter_map(|k| k.strip_prefix("price.")) {
                let level: Option<String> = tx
                    .query_row("SELECT id FROM price_levels WHERE code = ?1", [code], |r| {
                        r.get(0)
                    })
                    .optional()?;
                let Some(level) = level else { continue };
                input.prices.retain(|p| p.price_level_id != level);
                input.prices.extend(
                    before
                        .prices
                        .iter()
                        .filter(|p| p.price_level_id == level)
                        .cloned(),
                );
            }
        }
    }
    products::update(tx, &ctx.actor(), id, input)?;
    if fields.is_some() && set("costPrice") {
        let imported: Money = field(saved, "costAfter")?;
        restore_cost(tx, ctx, job_id, id, imported, before.cost_price)?;
    }
    if set("brand") || set("supplierRef") {
        let was: ProductExtrasInput = field(saved, "extras")?;
        let mut extras_input = ProductExtrasInput::from(&extras::get(tx, id)?);
        if set("brand") {
            extras_input.brand = was.brand;
        }
        if set("supplierRef") {
            extras_input.supplier_ref = was.supplier_ref;
        }
        // `set_quiet` refreshes the searchable text from the restored extras.
        extras::set_quiet(tx, id, extras_input)?;
    }
    Ok(())
}

/// Reverses one recorded change of job `job_id`.
pub(crate) fn undo_row(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    job_id: &str,
    entity: ImportEntity,
    id: &str,
    action: &str,
    before: Option<&str>,
) -> Result<(), AppError> {
    let actor = ctx.actor();
    let saved: Value = match before {
        Some(text) => serde_json::from_str(text)?,
        None => Value::Null,
    };
    match action {
        "created" => match entity {
            ImportEntity::Products => products::archive(tx, &actor, id),
            ImportEntity::Categories => categories::archive(tx, &actor, id),
            ImportEntity::Customers => customers::archive(tx, &actor, id),
            ImportEntity::Suppliers => suppliers::archive(tx, &actor, id),
            _ => Err(AppError::Internal(format!(
                "no created rows in a {} import",
                entity.as_str()
            ))),
        },
        "updated" if saved.get("archivedBySync") == Some(&Value::Bool(true)) => {
            products::restore(tx, &actor, id)
        }
        "updated" => match entity {
            ImportEntity::Products => undo_product(tx, ctx, job_id, id, &saved),
            ImportEntity::Categories => {
                categories::update(tx, &actor, id, serde_json::from_value(saved)?).map(|_| ())
            }
            ImportEntity::Customers => {
                customers::update(tx, &actor, &ctx.pack, id, serde_json::from_value(saved)?)
                    .map(|_| ())
            }
            ImportEntity::Suppliers => {
                suppliers::update(tx, &actor, &ctx.pack, id, serde_json::from_value(saved)?)
                    .map(|_| ())
            }
            _ => Err(AppError::Internal(format!(
                "no updated rows in a {} import",
                entity.as_str()
            ))),
        },
        "posted" => match saved.get("posting").and_then(Value::as_str) {
            Some("stock") => {
                let qty: Qty = field(&saved, "qty")?;
                let product_id: String = field(&saved, "productId")?;
                let input = AdjustInput {
                    product_id: product_id.clone(),
                    location_id: Some(field(&saved, "locationId")?),
                    qty_delta: Qty(-qty.0),
                    reason: AdjustReason::Correction,
                    unit_cost: None,
                    note: Some("import undo".into()),
                };
                stock::adjust(tx, &actor, input, ctx.now)?;
                // The CMUP the opening stock blended in goes back with it (records written
                // before the costs were kept have none to restore).
                let costs: (Option<Money>, Option<Money>) =
                    (field(&saved, "costBefore")?, field(&saved, "costAfter")?);
                match costs {
                    (Some(previous), Some(imported)) => {
                        restore_cost(tx, ctx, job_id, &product_id, imported, previous)
                    }
                    _ => Ok(()),
                }
            }
            Some(kind @ ("customer_opening" | "supplier_opening")) => {
                let side = if kind == "customer_opening" {
                    Side::Customer
                } else {
                    Side::Supplier
                };
                let amount: Money = field(&saved, "amount")?;
                let entry_id: String = field(&saved, "entryId")?;
                let party: String = field(&saved, "partyId")?;
                let counter = NewEntry::new(&party, EntryKind::Adjustment, Money(-amount.0))
                    .referencing("opening_undo", &entry_id);
                ledger::append(tx, side, &actor, &counter, &ctx.business_day(), ctx.now).map(|_| ())
            }
            Some("count") => {
                let before: Option<i64> = field(&saved, "before")?;
                let expected: Option<i64> = field(&saved, "expectedBefore")?;
                let counted_at: Option<i64> = field(&saved, "countedAtBefore")?;
                let count_id: String = field(&saved, "countId")?;
                let product_id: String = field(&saved, "productId")?;
                // The line as it was: its count, the level that count is compared with, when.
                tx.execute(
                    "UPDATE stock_count_lines
                        SET counted_qty = ?3,
                            counted_at = CASE WHEN ?3 IS NULL THEN NULL ELSE COALESCE(?5, counted_at) END,
                            expected_qty = COALESCE(?4, expected_qty)
                      WHERE count_id = ?1 AND product_id = ?2",
                    params![count_id, product_id, before, expected, counted_at],
                )
                .map_err(|_| AppError::conflict("import_undo_not_allowed"))?;
                Ok(())
            }
            _ => Err(AppError::Internal("unknown import posting".into())),
        },
        other => Err(AppError::Internal(format!("unknown import action {other}"))),
    }
}
