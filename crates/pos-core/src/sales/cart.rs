//! The cart and its pricing (spec §7.1 steps 2-3, §7.4; SAL-016/017/018/020/048/057,
//! PRC-005, PRM-001/004): catalogue prices at the chosen level, price overrides and open items
//! gated by `sales.price_override`, discounts gated by `sales.discount` and the user's limit,
//! quantities checked against the unit's decimals. Reads only: the live totals of the sale
//! screen and the sale itself price the same way.

use crate::auth::Permission;
use crate::catalog::products::{self, ProductKind};
use crate::catalog::{NAME_MAX, breaks, clean_opt, extras, price_levels, tax_rates};
use crate::company::{self, PriceMode, TaxRegime};
use crate::customers::{self, Customer};
use crate::error::AppError;
use crate::lang::Lang;
use crate::money::{Money, Qty, Rate};
use crate::payments::{self, PaymentInput, SettleRules, Settlement};
use crate::promotions::{self, AutoDiscount, LineFacts, PricingRules};
use crate::tax::tva::{self, Discount, LineAmounts, LineInput, Totals};
use crate::till::TillContext;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use specta::Type;

/// One line as the cashier built it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CartLine {
    /// `None` for an open item typed at the till (SAL-018).
    pub product_id: Option<String>,
    /// The open item's name; ignored for catalogue products.
    pub description: Option<String>,
    pub qty: Qty,
    /// A price override (catalogue product) or the open item's price.
    pub unit_price: Option<Money>,
    pub discount: Discount,
    /// The open item's tax rate; ignored for catalogue products.
    pub tax_rate_id: Option<String>,
    pub note: Option<String>,
    /// The unit price shown when the line was added: a held cart recalled later reports the
    /// lines whose price changed since (spec §7.4).
    pub expected_unit_price: Option<Money>,
    /// A weighed package (a scale label, the scale panel): a line of its own that the till never
    /// merges another scan into, kept when the ticket is held (VRT-010). Pricing ignores it;
    /// carts held before it read as `false`.
    #[serde(default)]
    pub separate: bool,
}

impl CartLine {
    /// `qty` of a catalogue product at its price.
    pub fn product(product_id: &str, qty: Qty) -> CartLine {
        CartLine {
            product_id: Some(product_id.to_owned()),
            description: None,
            qty,
            unit_price: None,
            discount: Discount::None,
            tax_rate_id: None,
            note: None,
            expected_unit_price: None,
            separate: false,
        }
    }

    /// An open item.
    pub fn open_item(
        description: &str,
        qty: Qty,
        unit_price: Money,
        tax_rate_id: &str,
    ) -> CartLine {
        CartLine {
            product_id: None,
            description: Some(description.to_owned()),
            qty,
            unit_price: Some(unit_price),
            discount: Discount::None,
            tax_rate_id: Some(tax_rate_id.to_owned()),
            note: None,
            expected_unit_price: None,
            separate: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Cart {
    pub lines: Vec<CartLine>,
    /// Default level when absent (PRC-005: switchable at the till).
    pub price_level_id: Option<String>,
    /// The identified customer (F3): his price level applies when the cart names none, and
    /// credit and store-credit tenders go to his account.
    pub customer_id: Option<String>,
    /// Language the document is printed in (names are taken in it).
    pub lang: Lang,
    pub note: Option<String>,
}

impl Cart {
    pub fn new(lang: Lang, lines: Vec<CartLine>) -> Cart {
        Cart {
            lines,
            price_level_id: None,
            customer_id: None,
            lang,
            note: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PricedLine {
    pub line_no: u32,
    pub product_id: Option<String>,
    pub kind: Option<ProductKind>,
    pub description: String,
    pub qty: Qty,
    pub unit_code: String,
    pub unit_price: Money,
    /// What one unit costs the customer, TTC, after the line's automatic discount (a promotion,
    /// the customer's discount): the price the customer screen and the pole display show, in an
    /// HT shop too (`unit_price` is in the shop's price mode).
    pub unit_price_ttc: Money,
    /// The price at the level, for catalogue products.
    pub catalogue_price: Option<Money>,
    pub price_overridden: bool,
    /// The price differs from `expected_unit_price` (held cart recalled after a price change).
    pub price_changed: bool,
    pub tax_rate_bp: Rate,
    pub amounts: LineAmounts,
    pub cost_price: Money,
    pub note: Option<String>,
    /// Promotion or customer discount the line received by itself (PRM, PRC-003).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub auto_discount: Option<AutoDiscount>,
    /// Sold below its HT cost (a warning for the back office, PRD-061).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[specta(optional)]
    pub below_cost: bool,
    /// A regulated staple (PRC-012): its ceiling and whether this price is above it (SAL-030).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub regulated: Option<crate::regulated::RegulatedFlag>,
    /// Priced at the price agreed with the cart's customer (PRC-003); no automatic discount.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    #[specta(optional)]
    pub customer_price: bool,
    /// The product's minimum order when this line is below it (wholesale).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub min_order: Option<Qty>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PricedCart {
    pub lines: Vec<PricedLine>,
    pub totals: Totals,
    pub price_level_id: String,
    pub price_mode: PriceMode,
    pub tax_regime: TaxRegime,
    pub lang: Lang,
    /// Sum of the quantities (SAL-048 item count).
    pub item_count: Qty,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub customer: Option<CartCustomer>,
}

/// The customer attached to the cart, as the cashier sees him before a credit sale (SAL-037).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CartCustomer {
    pub id: String,
    pub code: String,
    pub name: String,
    pub phone: Option<String>,
    pub balance: Money,
    pub credit_limit: Option<Money>,
    pub available_credit: Option<Money>,
    pub store_credit: Money,
    pub is_blocked: bool,
    pub discount_bp: Rate,
}

impl CartCustomer {
    fn of(c: &Customer) -> CartCustomer {
        CartCustomer {
            id: c.id.clone(),
            code: c.code.clone(),
            name: c.name.clone(),
            phone: c.phone.clone(),
            balance: c.balance,
            credit_limit: c.credit_limit,
            available_credit: c.available_credit,
            store_credit: c.store_credit,
            is_blocked: c.is_blocked,
            discount_bp: c.discount_bp,
        }
    }
}

/// The cart's customer, or `customerId`/`customer_not_found` when unknown or archived.
fn cart_customer(conn: &Connection, id: Option<&str>) -> Result<Option<Customer>, AppError> {
    id.map(|id| customers::selectable(conn, id)).transpose()
}

/// Live totals with the payment being typed (SAL-048, SAL-057 live timbre).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PaymentPreview {
    pub cart: PricedCart,
    pub settlement: Settlement,
}

/// A recalled cart priced again: `cart` carries the current prices as expected prices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RepricedCart {
    pub cart: Cart,
    pub priced: PricedCart,
    /// `line_no` of the lines whose price changed.
    pub changed_lines: Vec<u32>,
}

/// Re-homes a validation error of line `i` under `lines.{i}.<field>`.
pub(crate) fn at_line(i: usize, err: AppError) -> AppError {
    match err {
        AppError::Validation { field, code } => {
            AppError::validation(&format!("lines.{i}.{field}"), &code)
        }
        AppError::NotFound { .. } | AppError::Conflict { .. } => {
            AppError::validation(&format!("lines.{i}.productId"), &err.code())
        }
        other => other,
    }
}

/// The settings that shape a settlement (`stamp_duty.mode`, `sales.cash_rounding`).
pub fn settle_rules(conn: &Connection) -> Result<SettleRules, AppError> {
    Ok(SettleRules {
        stamp_mode: crate::settings::get(conn, "stamp_duty.mode")?,
        cash_rounding: Money(crate::settings::get::<i64>(conn, "sales.cash_rounding")?),
        store_credit: Money::ZERO,
    })
}

/// What pricing enforces. A preview and a sale refuse what the operator may not do; the journal
/// of a line void or a cancelled cart only values the lines as they stand (AUD-001): a line the
/// cashier may not sell, or whose product was archived meanwhile, is still worth what it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Check {
    Rights,
    ValueOnly,
}

/// What every line of a cart is priced with.
struct Terms<'a> {
    lang: Lang,
    mode: PriceMode,
    level_id: &'a str,
    day: &'a str,
    rules: &'a PricingRules<'a>,
    check: Check,
    /// The lines of a validated quote or proforma the cart keeps (see [`price_cart_offered`]).
    offered: &'a [CartLine],
    /// The cart's customer, for the prices agreed with him (PRC-003).
    customer_id: Option<&'a str>,
    /// The wholesale module is on: agreed prices and minimum orders apply.
    wholesale: bool,
}

/// A line typed exactly as an offered one: same product (or open item), price and discount.
fn agreed(offered: &[CartLine], line: &CartLine) -> bool {
    offered.iter().any(|o| {
        o.product_id == line.product_id
            && o.unit_price.is_some()
            && o.unit_price == line.unit_price
            && o.discount == line.discount
            && (o.product_id.is_some()
                || (o.description == line.description && o.tax_rate_id == line.tax_rate_id))
    })
}

/// PRM-002: a whole-order discount in dinars spread over the lines in proportion to their gross
/// amounts, the last line taking the rest, so the lines add up to it exactly. The returned cart
/// carries the per-line amounts; `price_cart` then checks them against the cashier's limit like
/// any line discount. Lines keep no automatic discount (a manual discount replaces it).
pub fn spread_discount(
    conn: &Connection,
    ctx: &TillContext,
    cart: &Cart,
    amount: Money,
) -> Result<Cart, AppError> {
    if !amount.is_positive() {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    let mut plain = cart.clone();
    for line in &mut plain.lines {
        line.discount = Discount::None;
    }
    let priced = price_cart(conn, ctx, &plain)?;
    let grosses: Vec<Money> = priced.lines.iter().map(|l| l.amounts.gross).collect();
    let total = Money::checked_sum(grosses.iter().copied())
        .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?;
    if amount > total {
        return Err(AppError::validation("amount", "discount_invalid"));
    }
    let mut left = amount;
    let last = grosses.len().saturating_sub(1);
    for (i, (line, gross)) in plain.lines.iter_mut().zip(grosses).enumerate() {
        let share = if i == last {
            left
        } else {
            amount
                .mul_div(gross.0, total.0)
                .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?
                .min(left)
        };
        left = left.checked_sub(share).unwrap_or(Money::ZERO);
        line.discount = if share.is_positive() {
            Discount::Amount(share)
        } else {
            Discount::None
        };
    }
    Ok(plain)
}

/// Prices every line and totals the cart; refuses what the operator may not do.
pub fn price_cart(
    conn: &Connection,
    ctx: &TillContext,
    cart: &Cart,
) -> Result<PricedCart, AppError> {
    price_cart_as(conn, ctx, cart, Check::Rights, &[], &[])
}

/// [`price_cart`] for a draft converted from a validated quote or proforma at its offered prices
/// (ORD-001): a line typed exactly as one of `offered` (product, price and discount) keeps those
/// terms. They were agreed when the offer was validated, so the operator's override, floor and
/// discount rights are not asked again and today's automatic discounts do not apply to it. Any
/// other line, or an offered line whose price or discount was changed, is priced as usual.
pub fn price_cart_offered(
    conn: &Connection,
    ctx: &TillContext,
    cart: &Cart,
    offered: &[CartLine],
) -> Result<PricedCart, AppError> {
    price_cart_as(conn, ctx, cart, Check::Rights, offered, &[])
}

/// Prices every line and totals the cart under the operator's rights, except the discount of
/// the lines `granted` marks (`granted[i]` for line `i`): a discount approved when it was given
/// (a restaurant comp, RST-008, checked then against the giver's right and limit and journaled
/// with its approval) is not asked again of whoever reprices or cashes the cart later. Every
/// other right is checked on those lines too.
pub(crate) fn price_cart_granted(
    conn: &Connection,
    ctx: &TillContext,
    cart: &Cart,
    granted: &[bool],
) -> Result<PricedCart, AppError> {
    price_cart_as(conn, ctx, cart, Check::Rights, &[], granted)
}

/// The cart's figures as it stands, for the journal of a void or a cancelled cart: the
/// operator's rights, archived products and retired tax rates are not checked. Never for a
/// sale or a preview (they use [`price_cart`]).
pub fn value_cart(
    conn: &Connection,
    ctx: &TillContext,
    cart: &Cart,
) -> Result<PricedCart, AppError> {
    price_cart_as(conn, ctx, cart, Check::ValueOnly, &[], &[])
}

fn price_cart_as(
    conn: &Connection,
    ctx: &TillContext,
    cart: &Cart,
    check: Check,
    offered: &[CartLine],
    granted: &[bool],
) -> Result<PricedCart, AppError> {
    if cart.lines.is_empty() {
        return Err(AppError::validation("lines", "cart_empty"));
    }
    // Valuing only: a customer archived since the cart was built still values it.
    let customer = match (check, cart.customer_id.as_deref()) {
        (Check::ValueOnly, Some(id)) => Some(customers::get(conn, id)?),
        (_, id) => cart_customer(conn, id)?,
    };
    let company = company::get(conn)?.ok_or_else(|| AppError::conflict("setup_required"))?;
    // PRC-002: the cart's level, else the customer's (an archived level falls back to the default).
    let level = match (
        &cart.price_level_id,
        customer.as_ref().and_then(|c| c.price_level_id.as_deref()),
    ) {
        (Some(id), _) => price_levels::active(conn, id)
            .map_err(|_| AppError::validation("priceLevelId", "price_level_not_found"))?,
        (None, Some(id)) => match price_levels::active(conn, id) {
            Ok(level) => level,
            Err(_) => price_levels::default_level(conn)?,
        },
        (None, None) => price_levels::default_level(conn)?,
    };
    let day = ctx.business_day();
    let promos = promotions::active_on(conn, &day)?;
    let rules = PricingRules {
        promos: &promos,
        customer_discount: customer.as_ref().map_or(Rate::ZERO, |c| c.discount_bp),
    };
    let terms = Terms {
        lang: cart.lang,
        mode: company.price_mode,
        level_id: &level.id,
        day: &day,
        rules: &rules,
        check,
        offered,
        customer_id: customer.as_ref().map(|c| c.id.as_str()),
        wholesale: crate::packs::enabled(conn, crate::packs::Module::Wholesale)?,
    };
    let mut lines = Vec::with_capacity(cart.lines.len());
    for (i, line) in cart.lines.iter().enumerate() {
        let discount_granted = granted.get(i).copied().unwrap_or(false);
        let priced =
            price_line(conn, ctx, &terms, line, discount_granted).map_err(|e| at_line(i, e))?;
        lines.push(PricedLine {
            line_no: u32::try_from(i + 1).unwrap_or(u32::MAX),
            ..priced
        });
    }
    let totals = tva::totals(lines.iter().map(|l| (l.tax_rate_bp, &l.amounts)))?;
    let item_count = lines
        .iter()
        .try_fold(Qty::ZERO, |acc, l| acc.checked_add(l.qty))
        .ok_or_else(|| AppError::validation("lines", "qty_invalid"))?;
    Ok(PricedCart {
        lines,
        totals,
        price_level_id: level.id,
        price_mode: company.price_mode,
        tax_regime: company.tax_regime,
        lang: cart.lang,
        item_count,
        customer: customer.as_ref().map(CartCustomer::of),
    })
}

/// Prices one line: the level price or a lower quantity break (PRC-004), an override within the
/// operator's rights and not below the floor (PRD-061), the manual discount within the limit
/// (PRM-004), else the best automatic discount (promotion or customer, PRC-003, PRM-005). An
/// open-price product sells at the typed price (SAL-019); a regulated staple is flagged, and
/// above its ceiling warns or is refused as `sales.regulated_policy` says (SAL-030).
/// Valuing only ([`value_cart`]) checks none of the operator's rights (override, floor, discount,
/// ceiling); a `discount_granted` line ([`price_cart_granted`]) skips the discount right only.
fn price_line(
    conn: &Connection,
    ctx: &TillContext,
    terms: &Terms<'_>,
    line: &CartLine,
    discount_granted: bool,
) -> Result<PricedLine, AppError> {
    let rights = terms.check == Check::Rights;
    // An offered line keeps the terms agreed on its quote: no right is asked for them again.
    let offered = agreed(terms.offered, line);
    let asks = rights && !offered;
    let note = clean_opt(line.note.as_deref(), "note", 200)?;
    if !line.qty.is_positive() {
        return Err(AppError::validation("qty", "qty_invalid"));
    }
    let (
        product_id,
        kind,
        category_id,
        description,
        unit_code,
        unit_price,
        catalogue_price,
        rate_id,
        cost_price,
        customer_price,
        min_order,
    ) = match &line.product_id {
        Some(id) => {
            let product = if rights {
                products::sellable(conn, id)?
            } else {
                products::get(conn, id)?
            };
            if !line.qty.fits_decimals(product.unit_decimals) {
                return Err(AppError::validation("qty", "qty_invalid"));
            }
            // VRT-140: a piece priced by weight costs its live price from the gold board, which
            // replaces the level price; a quantity break never applies to it.
            let catalogue = match crate::jewellery::live_price(conn, &product.id)? {
                Some(live) => live,
                None => {
                    let level_price = product.price_for(terms.level_id);
                    // PRC-004: a quantity break lower than the level price wins.
                    match breaks::best_price(conn, &product.id, terms.level_id, line.qty)? {
                        Some(b) if b < level_price => b,
                        _ => level_price,
                    }
                }
            };
            // PRC-003: a price agreed with the cart's customer replaces the level and break prices.
            let agreed_price = match (terms.wholesale, terms.customer_id) {
                (true, Some(customer_id)) => {
                    crate::wholesale::price_for(conn, customer_id, &product.id)?
                }
                _ => None,
            };
            let catalogue = agreed_price.unwrap_or(catalogue);
            let min_order = if terms.wholesale {
                crate::wholesale::min_order(conn, &product.id)?.filter(|m| line.qty < *m)
            } else {
                None
            };
            if rights
                && min_order.is_some()
                && crate::wholesale::min_order_policy(conn)?
                    == crate::wholesale::MinOrderPolicy::Block
            {
                return Err(AppError::validation("qty", "below_min_order"));
            }
            // SAL-019, VRT-002: a department key or a top-up is priced when it is sold; the
            // typed price is its price, not an override, and no floor applies to it.
            let open_price = crate::catalog::departments::is_open_price(conn, &product.id)?;
            let unit_price = if open_price {
                line.unit_price
                    .filter(|p| p.is_positive())
                    .ok_or_else(|| AppError::validation("unitPrice", "required"))?
            } else {
                line.unit_price.unwrap_or(catalogue)
            };
            let catalogue = if open_price { unit_price } else { catalogue };
            if asks && unit_price != catalogue && !ctx.allows(Permission::SalesPriceOverride) {
                return Err(AppError::validation(
                    "unitPrice",
                    "price_override_not_allowed",
                ));
            }
            // PRD-061: below the floor needs the right to set prices or a manager's approval,
            // whatever the price's source (typed, a level picked on the cart, a quantity break).
            let floor = extras::get(conn, &product.id)?.min_price;
            if asks
                && !open_price
                && floor.is_some_and(|f| unit_price < f)
                && !ctx.session.has(Permission::ProductsPrice)
                && ctx.approved_by(Permission::SalesPriceOverride).is_none()
            {
                return Err(AppError::validation("unitPrice", "price_below_floor"));
            }
            (
                Some(product.id.clone()),
                Some(product.kind),
                product.category_id.clone(),
                product.name_in(terms.lang).to_owned(),
                product.unit_code.clone(),
                unit_price,
                Some(catalogue),
                product.tax_rate_id.clone(),
                product.cost_price,
                agreed_price.is_some() && !open_price,
                min_order,
            )
        }
        None => {
            if asks && !ctx.allows(Permission::SalesPriceOverride) {
                return Err(AppError::validation("productId", "free_line_not_allowed"));
            }
            let description = clean_opt(line.description.as_deref(), "description", NAME_MAX)?
                .ok_or_else(|| AppError::validation("description", "required"))?;
            let unit_price = line
                .unit_price
                .ok_or_else(|| AppError::validation("unitPrice", "required"))?;
            let rate_id = line
                .tax_rate_id
                .clone()
                .ok_or_else(|| AppError::validation("taxRateId", "required"))?;
            (
                None,
                None,
                None,
                description,
                String::new(),
                unit_price,
                None,
                rate_id,
                Money::ZERO,
                false,
                None,
            )
        }
    };
    let rate = tax_rates::get(conn, &rate_id)
        .map_err(|_| AppError::validation("taxRateId", "tax_rate_not_found"))?;
    if rights && !rate.usable_on(terms.day) {
        return Err(AppError::validation("taxRateId", "tax_rate_inactive"));
    }
    // PRC-012, SAL-030: a regulated staple whose unit costs the consumer more than its ceiling
    // (TTC, whatever the price mode) warns or is refused.
    let regulated = match &product_id {
        Some(id) => {
            crate::regulated::check(conn, &ctx.pack, id, terms.mode, unit_price, rate.rate_bp)?
        }
        None => None,
    };
    if rights
        && regulated.as_ref().is_some_and(|f| f.above)
        && crate::regulated::policy(conn)? == crate::regulated::RegulatedPolicy::Block
    {
        return Err(AppError::validation("unitPrice", "price_above_ceiling"));
    }
    let input = |discount: Discount| LineInput {
        unit_price,
        qty: line.qty,
        discount,
        rate: rate.rate_bp,
    };
    let mut amounts = tva::compute_line(terms.mode, &input(line.discount))?;
    if asks && !discount_granted && amounts.discount_amount.is_positive() {
        if !ctx.allows(Permission::SalesDiscount) {
            return Err(AppError::validation("discount", "discount_not_allowed"));
        }
        let ceiling = amounts
            .gross
            .apply_rate(ctx.discount_limit())
            .ok_or_else(|| AppError::validation("discount", "discount_invalid"))?;
        if amounts.discount_amount > ceiling {
            return Err(AppError::validation("discount", "discount_above_limit"));
        }
    }
    let overridden = catalogue_price.is_some_and(|c| c != unit_price);
    let mut auto_discount = None;
    // A manual discount, a price override or a price agreed with the customer replaces the
    // automatic discounts; an offered line keeps the discount of its offer.
    if let (Some(pid), Discount::None, false, false) = (
        &product_id,
        line.discount,
        overridden,
        offered || customer_price,
    ) {
        let facts = LineFacts {
            product_id: pid,
            category_id: category_id.as_deref(),
            unit_price,
            qty: line.qty,
            gross: amounts.gross,
        };
        if let Some(auto) = promotions::best_discount(terms.rules, &facts, terms.lang) {
            amounts = tva::compute_line(terms.mode, &input(Discount::Amount(auto.amount)))?;
            auto_discount = Some(auto);
        }
    }
    let below_cost = cost_price.is_positive()
        && cost_price
            .mul_qty(line.qty)
            .is_some_and(|cost| amounts.total_ht < cost);
    // One unit TTC; after an automatic discount, the line's TTC per unit (to the centime).
    let unit_price_ttc = match &auto_discount {
        None => {
            tva::compute_line(
                terms.mode,
                &LineInput {
                    qty: Qty::ONE,
                    ..input(Discount::None)
                },
            )?
            .total_ttc
        }
        Some(_) => amounts
            .total_ttc
            .mul_div(Qty::ONE.0, line.qty.0)
            .ok_or_else(|| AppError::validation("qty", "qty_invalid"))?,
    };
    Ok(PricedLine {
        line_no: 0,
        product_id,
        kind,
        description,
        qty: line.qty,
        unit_code,
        unit_price,
        unit_price_ttc,
        catalogue_price,
        price_overridden: overridden,
        price_changed: line.expected_unit_price.is_some_and(|e| e != unit_price),
        tax_rate_bp: rate.rate_bp,
        amounts,
        cost_price,
        note,
        auto_discount,
        below_cost,
        regulated,
        customer_price,
        min_order,
    })
}

/// Prices a recalled cart against today's prices and refreshes the expected prices, listing
/// the lines whose price moved (spec §7.4 "re-price with a notice").
pub fn reprice_cart(
    conn: &Connection,
    ctx: &TillContext,
    cart: &Cart,
) -> Result<RepricedCart, AppError> {
    let priced = price_cart(conn, ctx, cart)?;
    let changed_lines = priced
        .lines
        .iter()
        .filter(|l| l.price_changed)
        .map(|l| l.line_no)
        .collect();
    let mut refreshed = cart.clone();
    for (line, p) in refreshed.lines.iter_mut().zip(&priced.lines) {
        line.expected_unit_price = Some(p.unit_price);
    }
    Ok(RepricedCart {
        cart: refreshed,
        priced,
        changed_lines,
    })
}

/// The cart's totals and the settlement of the tenders typed so far, duty included.
pub fn preview_payment(
    conn: &Connection,
    ctx: &TillContext,
    cart: &Cart,
    tenders: &[PaymentInput],
) -> Result<PaymentPreview, AppError> {
    let priced = price_cart(conn, ctx, cart)?;
    let settlement = payments::preview(
        &ctx.pack,
        priced.totals.total_ttc,
        tenders,
        settle_rules(conn)?,
    )?;
    Ok(PaymentPreview {
        cart: priced,
        settlement,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::session::OverrideGrant;
    use crate::catalog::products::{LevelPrice, ProductInput};
    use crate::catalog::{UNIT_KG_ID, WHOLESALE_LEVEL_ID};
    use crate::testkit::{NOON, Shop, shop};

    fn cashier(shop: &Shop) -> TillContext {
        TillContext::new(shop.cashier.clone(), shop.pack.clone(), NOON)
    }

    fn price(shop: &Shop, ctx: TillContext, cart: Cart) -> Result<PricedCart, AppError> {
        shop.db.read(move |c| price_cart(c, &ctx, &cart))
    }

    fn field_code(e: AppError) -> (String, String) {
        let p = e.payload();
        (p.field.unwrap_or_default(), p.code)
    }

    /// The customer screens show what one unit costs the customer: TTC in an HT shop, after the
    /// line's automatic discount (VRT-010 per kg, SAL-047).
    #[test]
    fn a_line_says_its_unit_price_ttc_after_its_automatic_discount() {
        let shop = crate::testkit::shop_with(crate::company::TaxRegime::Reel, PriceMode::Ht);
        let mut tomato = ProductInput::simple("Tomates", UNIT_KG_ID, &shop.tva19, Money(10_000));
        tomato.is_weighed = true;
        let tomato = shop.product_with(tomato);
        let weighed = || Cart::new(Lang::Fr, vec![CartLine::product(&tomato.id, Qty(1_250))]);
        let line = price(&shop, cashier(&shop), weighed())
            .unwrap()
            .lines
            .remove(0);
        assert_eq!(
            (line.unit_price, line.unit_price_ttc, line.amounts.total_ttc),
            (Money(10_000), Money(11_900), Money(14_875)),
            "100,00 HT the kg is 119,00 TTC"
        );
        let fidele = {
            let mut input = crate::customers::CustomerInput::named("Client fidèle");
            input.discount_bp = Rate(1_000);
            let (pack, actor) = (std::sync::Arc::clone(&shop.pack), shop.owner.actor());
            shop.db
                .write(move |tx| customers::create(tx, &actor, &pack, input))
                .unwrap()
        };
        let mut cart = weighed();
        cart.customer_id = Some(fidele.id);
        let line = price(&shop, cashier(&shop), cart).unwrap().lines.remove(0);
        assert!(line.auto_discount.is_some());
        assert_eq!(
            line.unit_price_ttc,
            Money(10_710),
            "119,00 less the customer's 10 %, what the kg costs him"
        );
    }

    #[test]
    fn catalogue_lines_at_the_level_price() {
        let shop = shop();
        let mut milk = ProductInput::simple(
            "Lait",
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva19,
            Money(11_900),
        );
        milk.name_ar = "حليب".into();
        milk.prices = vec![LevelPrice {
            price_level_id: WHOLESALE_LEVEL_ID.into(),
            price: Money(10_710),
        }];
        let milk = shop.product_with(milk);
        let cart = Cart::new(Lang::Ar, vec![CartLine::product(&milk.id, Qty(2_000))]);
        let p = price(&shop, cashier(&shop), cart.clone()).unwrap();
        assert_eq!(
            (p.lines[0].description.as_str(), p.lines[0].unit_price),
            ("حليب", Money(11_900))
        );
        assert_eq!(
            (p.totals.total_ttc, p.totals.tax_total, p.item_count),
            (Money(23_800), Money(3_800), Qty(2_000))
        );
        let wholesale = Cart {
            price_level_id: Some(WHOLESALE_LEVEL_ID.into()),
            ..cart
        };
        let p = price(&shop, cashier(&shop), wholesale).unwrap();
        assert_eq!(
            (p.lines[0].unit_price, p.lines[0].price_overridden),
            (Money(10_710), false)
        );
        assert_eq!(p.price_level_id, WHOLESALE_LEVEL_ID);
    }

    #[test]
    fn weighed_lines_accept_grams_and_pieces_do_not() {
        let shop = shop();
        let mut tomato = ProductInput::simple("Tomate", UNIT_KG_ID, &shop.tva9, Money(12_000));
        tomato.is_weighed = true;
        let tomato = shop.product_with(tomato);
        let bread = shop.product("Pain", Money(1_000), &shop.exempt);
        let p = price(
            &shop,
            cashier(&shop),
            Cart::new(Lang::Fr, vec![CartLine::product(&tomato.id, Qty(255))]),
        )
        .unwrap();
        assert_eq!(p.totals.total_ttc, Money(3_060));
        let err = price(
            &shop,
            cashier(&shop),
            Cart::new(Lang::Fr, vec![CartLine::product(&bread.id, Qty(1_500))]),
        )
        .unwrap_err();
        assert_eq!(
            field_code(err),
            ("lines.0.qty".into(), "qty_invalid".into())
        );
    }

    #[test]
    fn overrides_open_items_and_discounts_need_permissions() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let mut over = CartLine::product(&milk.id, Qty(1_000));
        over.unit_price = Some(Money(9_000));
        let err = price(
            &shop,
            cashier(&shop),
            Cart::new(Lang::Fr, vec![over.clone()]),
        )
        .unwrap_err();
        assert_eq!(
            field_code(err),
            (
                "lines.0.unitPrice".into(),
                "price_override_not_allowed".into()
            )
        );
        let open = CartLine::open_item("Légumes", Qty(1_000), Money(5_000), &shop.exempt);
        let err = price(
            &shop,
            cashier(&shop),
            Cart::new(Lang::Fr, vec![open.clone()]),
        )
        .unwrap_err();
        assert_eq!(
            field_code(err),
            ("lines.0.productId".into(), "free_line_not_allowed".into())
        );
        let owner = TillContext::new(shop.owner.clone(), shop.pack.clone(), NOON);
        let p = price(&shop, owner, Cart::new(Lang::Fr, vec![over.clone(), open])).unwrap();
        assert!(p.lines[0].price_overridden);
        assert_eq!(
            (
                p.lines[1].description.as_str(),
                p.lines[1].product_id.clone()
            ),
            ("Légumes", None)
        );
        // The built-in cashier role has no `sales.discount`: refused.
        let mut ten = CartLine::product(&milk.id, Qty(1_000));
        ten.discount = Discount::Percent(Rate(1_000));
        let err = price(
            &shop,
            cashier(&shop),
            Cart::new(Lang::Fr, vec![ten.clone()]),
        )
        .unwrap_err();
        assert_eq!(
            field_code(err),
            ("lines.0.discount".into(), "discount_not_allowed".into())
        );
        // A cashier whose role grants it may discount up to their limit (10 %).
        let discounting = || {
            let mut ctx = cashier(&shop);
            ctx.session.permissions.push(Permission::SalesDiscount);
            ctx
        };
        let p = price(&shop, discounting(), Cart::new(Lang::Fr, vec![ten])).unwrap();
        assert_eq!(p.totals.discount_total, Money(1_000));
        let mut fifteen = CartLine::product(&milk.id, Qty(1_000));
        fifteen.discount = Discount::Amount(Money(1_500));
        let err = price(
            &shop,
            discounting(),
            Cart::new(Lang::Fr, vec![fifteen.clone()]),
        )
        .unwrap_err();
        assert_eq!(
            field_code(err),
            ("lines.0.discount".into(), "discount_above_limit".into())
        );
        // A manager's approval lifts the limit, even for a role without the permission.
        let grant = OverrideGrant {
            approver_id: shop.owner.user_id.clone(),
            approver_name: "Amine".into(),
            permission: Permission::SalesDiscount,
            granted_at: NOON,
            discount_limit_bp: 10_000,
        };
        let approved = cashier(&shop).with_approval(grant);
        let p = price(&shop, approved, Cart::new(Lang::Fr, vec![fifteen])).unwrap();
        assert_eq!(p.totals.total_ttc, Money(8_500));
    }

    #[test]
    fn unusable_products_and_carts_are_refused() {
        let shop = shop();
        let err = price(&shop, cashier(&shop), Cart::new(Lang::Fr, Vec::new())).unwrap_err();
        assert_eq!(field_code(err), ("lines".into(), "cart_empty".into()));
        let err = price(
            &shop,
            cashier(&shop),
            Cart::new(Lang::Fr, vec![CartLine::product("nope", Qty(1_000))]),
        )
        .unwrap_err();
        assert_eq!(
            field_code(err),
            ("lines.0.productId".into(), "product_not_found".into())
        );
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let with_customer = Cart {
            customer_id: Some("c1".into()),
            ..Cart::new(Lang::Fr, vec![CartLine::product(&milk.id, Qty(1_000))])
        };
        assert_eq!(
            price(&shop, cashier(&shop), with_customer).unwrap_err(),
            AppError::validation("customerId", "customer_not_found")
        );
    }

    #[test]
    fn valuing_a_cart_ignores_rights_and_archiving() {
        // The journal of a void values the line the cashier removes, even one they could not
        // sell (a refused override, discount or open item) or whose product was archived since.
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let oil = shop.product("Huile", Money(60_000), &shop.tva19);
        let mut over = CartLine::product(&milk.id, Qty(1_000));
        over.unit_price = Some(Money(9_000));
        let mut half = CartLine::product(&milk.id, Qty(2_000));
        half.discount = Discount::Percent(Rate(5_000));
        let open = CartLine::open_item("Légumes", Qty(1_000), Money(5_000), &shop.exempt);
        let (actor, id) = (shop.owner.actor(), oil.id.clone());
        shop.db
            .write(move |tx| products::archive(tx, &actor, &id))
            .unwrap();
        let cart = Cart::new(
            Lang::Fr,
            vec![over, half, open, CartLine::product(&oil.id, Qty(1_000))],
        );
        assert!(price(&shop, cashier(&shop), cart.clone()).is_err());
        let ctx = cashier(&shop);
        let v = shop.db.read(move |c| value_cart(c, &ctx, &cart)).unwrap();
        let lines: Vec<_> = v
            .lines
            .iter()
            .map(|l| (l.description.as_str(), l.amounts.total_ttc))
            .collect();
        assert_eq!(
            lines,
            [
                ("Lait", Money(9_000)),
                ("Lait", Money(10_000)),
                ("Légumes", Money(5_000)),
                ("Huile", Money(60_000))
            ]
        );
        assert_eq!(v.totals.total_ttc, Money(84_000));
    }

    #[test]
    fn repricing_reports_changed_prices() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let mut line = CartLine::product(&milk.id, Qty(1_000));
        line.expected_unit_price = Some(Money(9_500));
        let ctx = cashier(&shop);
        let cart = Cart::new(Lang::Fr, vec![line]);
        let r = shop.db.read(move |c| reprice_cart(c, &ctx, &cart)).unwrap();
        assert_eq!(r.changed_lines, [1]);
        assert_eq!(r.cart.lines[0].expected_unit_price, Some(Money(10_000)));
    }

    #[test]
    fn preview_shows_the_duty_and_the_change() {
        let shop = shop();
        let milk = shop.product("Lait", Money(45_000), &shop.tva19);
        let ctx = cashier(&shop);
        let cart = Cart::new(Lang::Fr, vec![CartLine::product(&milk.id, Qty(1_000))]);
        let p = shop
            .db
            .read(move |c| preview_payment(c, &ctx, &cart, &[PaymentInput::cash(Money(100_000))]))
            .unwrap();
        assert_eq!(
            (
                p.settlement.stamp_duty,
                p.settlement.total_due,
                p.settlement.change
            ),
            (Money(500), Money(45_500), Money(54_500))
        );
    }
}
