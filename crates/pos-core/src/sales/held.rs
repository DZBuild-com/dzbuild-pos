//! Held tickets (spec §7.4, SAL-003) and the audit of cart edits that never become a document
//! (SAL-040/041, AUD-001: line voids and cancelled carts). Holding touches neither stock nor
//! cash; a recalled cart is priced again with today's prices.

use super::cart::{self, Cart, CartLine, RepricedCart};
use crate::audit;
use crate::catalog::{clean_opt, products};
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::{Money, Qty};
use crate::till::TillContext;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HeldCart {
    pub id: String,
    pub terminal_id: String,
    pub user_id: String,
    /// Customer name or table, typed by the cashier.
    pub label: String,
    pub cart: Cart,
    pub created_at: i64,
}

/// A cart edit worth a line in the journal, as the till asks for it: the cart on screen (it
/// lives in the webview), never a figure. The backend values it itself, so a journal row cannot
/// carry a name or an amount kept from an older preview (AUD-001, RPT-018).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum CartEventInput {
    /// Line `index` (from 0) leaves the cart before payment (`sales.void_line`).
    LineVoided { cart: Cart, index: u32 },
    /// The whole cart is abandoned (`sales.void_line`).
    CartCancelled { cart: Cart },
    /// Line `index` (from 0) goes down to `qty` (milli-units), a partial void
    /// (`sales.void_line`): the cart is sent as it stands, before the change.
    QtyReduced { cart: Cart, index: u32, qty: Qty },
}

/// The journal's record of a [`CartEventInput`], valued by the backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum CartEvent {
    /// A line removed from the cart before payment (`sales.void_line`).
    LineVoided {
        product_id: Option<String>,
        description: String,
        qty: Qty,
        amount: Money,
    },
    /// The whole cart abandoned (`sales.void_line`).
    CartCancelled { lines: u32, total: Money },
    /// A line's quantity lowered before payment (`sales.void_line`): what left the cart.
    QtyReduced {
        product_id: Option<String>,
        description: String,
        qty_before: Qty,
        qty_after: Qty,
        removed_qty: Qty,
        amount: Money,
    },
}

/// id, terminal, user, label, JSON payload, creation time.
type HeldRow = (String, String, String, String, String, i64);

fn row_to_held(r: &rusqlite::Row<'_>) -> rusqlite::Result<HeldRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
    ))
}

fn decode(row: HeldRow) -> Result<HeldCart, AppError> {
    let (id, terminal_id, user_id, label, payload, created_at) = row;
    Ok(HeldCart {
        cart: serde_json::from_str(&payload)?,
        id,
        terminal_id,
        user_id,
        label,
        created_at,
    })
}

/// Puts a cart aside on this terminal (`sales.hold`). The cart is priced first so that a cart
/// the cashier could not sell is not kept either.
pub fn hold(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    label: &str,
    cart: Cart,
) -> Result<HeldCart, AppError> {
    let label = clean_opt(Some(label), "label", 60)?.unwrap_or_default();
    let repriced = cart::reprice_cart(tx, ctx, &cart)?;
    let id = new_id();
    tx.execute(
        "INSERT INTO held_carts (id, terminal_id, user_id, label, payload, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            ctx.session.terminal_id,
            ctx.session.user_id,
            label,
            serde_json::to_string(&repriced.cart)?,
            ctx.now
        ],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "sale.held",
            "held_cart",
            Some(&id),
            json!({ "label": label, "lines": repriced.cart.lines.len(), "total": repriced.priced.totals.total_ttc }),
        ),
    )?;
    get(tx, &id)
}

pub fn get(conn: &Connection, id: &str) -> Result<HeldCart, AppError> {
    let row = conn
        .query_row(
            "SELECT id, terminal_id, user_id, label, payload, created_at FROM held_carts WHERE id = ?1",
            [id],
            row_to_held,
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("held_cart"))?;
    decode(row)
}

/// Held carts of a terminal, oldest first.
pub fn list(conn: &Connection, terminal_id: &str) -> Result<Vec<HeldCart>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, terminal_id, user_id, label, payload, created_at FROM held_carts
         WHERE terminal_id = ?1 ORDER BY created_at, rowid",
    )?;
    let rows = stmt
        .query_map([terminal_id], row_to_held)?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter().map(decode).collect()
}

fn take(tx: &Transaction<'_>, ctx: &TillContext, id: &str) -> Result<HeldCart, AppError> {
    let held = get(tx, id)?;
    if held.terminal_id != ctx.session.terminal_id {
        return Err(AppError::not_found("held_cart"));
    }
    tx.execute("DELETE FROM held_carts WHERE id = ?1", [id])?;
    Ok(held)
}

/// Takes a held cart back (it leaves the list) and prices it again: `changed_lines` are the
/// lines whose price moved since it was held.
pub fn recall(tx: &Transaction<'_>, ctx: &TillContext, id: &str) -> Result<RepricedCart, AppError> {
    let held = take(tx, ctx, id)?;
    let repriced = cart::reprice_cart(tx, ctx, &held.cart)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "sale.recalled",
            "held_cart",
            Some(id),
            json!({ "label": held.label, "changedLines": repriced.changed_lines }),
        ),
    )?;
    Ok(repriced)
}

/// Throws a held cart away: a whole ticket cancelled, so the command checks `sales.void_line`
/// and the journal row carries the cart's value today and the approvals ([`cart::value_cart`];
/// a cart that can no longer be valued is still thrown away, with no total).
pub fn discard(tx: &Transaction<'_>, ctx: &TillContext, id: &str) -> Result<(), AppError> {
    let held = take(tx, ctx, id)?;
    let total = cart::value_cart(tx, ctx, &held.cart)
        .ok()
        .map(|valued| valued.totals.total_ttc);
    audit::append(
        tx,
        &ctx.actor().event(
            "sale.held_discarded",
            "held_cart",
            Some(id),
            json!({
                "label": held.label,
                "lines": held.cart.lines.len(),
                "total": total,
                "approvals": ctx.approvers(),
            }),
        ),
    )?;
    Ok(())
}

/// Line `index` of `cart`, or `value_invalid` when the cart has no such line.
fn line_at(cart: &Cart, index: u32) -> Result<&CartLine, AppError> {
    usize::try_from(index)
        .ok()
        .and_then(|i| cart.lines.get(i))
        .ok_or_else(|| AppError::validation("index", "value_invalid"))
}

/// Line `index` of `cart` valued alone, at the quantity it has in `cart`.
fn value_line(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    cart: &Cart,
    index: u32,
) -> Result<cart::PricedLine, AppError> {
    let line = line_at(cart, index)?.clone();
    let alone = Cart {
        lines: vec![line],
        ..cart.clone()
    };
    cart::value_cart(tx, ctx, &alone)?
        .lines
        .into_iter()
        .next()
        .ok_or_else(|| AppError::validation("index", "value_invalid"))
}

/// Whether `err` only means a line cannot be priced: an open-price product rung without its price,
/// a free line without a tax rate, a product archived meanwhile. A void or a cancellation is
/// journaled anyway, with what the cart still says; a database, I/O or internal failure is not
/// swallowed.
fn unpriceable(err: &AppError) -> bool {
    matches!(
        err,
        AppError::Validation { .. } | AppError::NotFound { .. } | AppError::Conflict { .. }
    )
}

/// What the journal keeps of a line the backend cannot price: its product, its name (the open
/// item's, else the catalogue's in the ticket's language) and the amount its typed price gives
/// (nothing when it has none).
fn described_line(
    conn: &Connection,
    cart: &Cart,
    line: &CartLine,
) -> (Option<String>, String, Money) {
    let description = line
        .description
        .clone()
        .or_else(|| {
            line.product_id
                .as_deref()
                .and_then(|id| products::get(conn, id).ok())
                .map(|product| product.name_in(cart.lang).to_owned())
        })
        .or_else(|| line.product_id.clone())
        .unwrap_or_default();
    let amount = line
        .unit_price
        .and_then(|price| price.mul_qty(line.qty))
        .unwrap_or(Money::ZERO);
    (line.product_id.clone(), description, amount)
}

/// The cart's total as far as it can be priced: a line the backend refuses is left out.
fn valueable_total(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    cart: &Cart,
) -> Result<Money, AppError> {
    let mut amounts = Vec::with_capacity(cart.lines.len());
    for index in 0..cart.lines.len() {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        match value_line(tx, ctx, cart, index) {
            Ok(line) => amounts.push(line.amounts.total_ttc),
            Err(err) if unpriceable(&err) => {}
            Err(err) => return Err(err),
        }
    }
    Ok(Money::checked_sum(amounts).unwrap_or(Money::ZERO))
}

/// Journals a line void, a lowered quantity or a cancelled cart (the command checks
/// `sales.void_line`) with the backend's figures: the voided line is valued alone (another line's
/// refusal cannot block the void), the cancelled cart as a whole, both as they stand
/// ([`cart::value_cart`]). A line that can no longer be priced — an open-price product rung
/// without its price — is still journaled, with its name, its quantity and the amount its typed
/// price gives: a cart that cannot be sold must stay voidable and cancellable (SAL-040/041,
/// AUD-001), as [`discard`] already does for a held ticket.
pub fn record_cart_event(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: CartEventInput,
) -> Result<(), AppError> {
    let event = match input {
        CartEventInput::LineVoided { cart, index } => {
            let line = line_at(&cart, index)?;
            let (product_id, description, amount) = match value_line(tx, ctx, &cart, index) {
                Ok(priced) => (
                    priced.product_id,
                    priced.description,
                    priced.amounts.total_ttc,
                ),
                Err(err) if unpriceable(&err) => described_line(tx, &cart, line),
                Err(err) => return Err(err),
            };
            CartEvent::LineVoided {
                product_id,
                description,
                qty: line.qty,
                amount,
            }
        }
        CartEventInput::CartCancelled { cart } => {
            let total = match cart::value_cart(tx, ctx, &cart) {
                Ok(valued) => valued.totals.total_ttc,
                Err(err) if unpriceable(&err) => valueable_total(tx, ctx, &cart)?,
                Err(err) => return Err(err),
            };
            CartEvent::CartCancelled {
                lines: u32::try_from(cart.lines.len()).unwrap_or(u32::MAX),
                total,
            }
        }
        CartEventInput::QtyReduced { cart, index, qty } => {
            let line = line_at(&cart, index)?;
            if !qty.is_positive() {
                return Err(AppError::validation("qty", "qty_invalid"));
            }
            if qty >= line.qty {
                return Err(AppError::validation("qty", "value_invalid"));
            }
            let amount = match value_line(tx, ctx, &cart, index) {
                Ok(before) => {
                    let mut lowered = cart.clone();
                    if let Some(line) = usize::try_from(index)
                        .ok()
                        .and_then(|i| lowered.lines.get_mut(i))
                    {
                        line.qty = qty;
                    }
                    match value_line(tx, ctx, &lowered, index) {
                        Ok(after) => before
                            .amounts
                            .total_ttc
                            .checked_sub(after.amounts.total_ttc)
                            .ok_or_else(|| AppError::validation("qty", "value_invalid"))?,
                        Err(err) if unpriceable(&err) => Money::ZERO,
                        Err(err) => return Err(err),
                    }
                }
                Err(err) if unpriceable(&err) => Money::ZERO,
                Err(err) => return Err(err),
            };
            let (product_id, description, _) = described_line(tx, &cart, line);
            CartEvent::QtyReduced {
                product_id,
                description,
                qty_before: line.qty,
                qty_after: qty,
                removed_qty: Qty(line.qty.0 - qty.0),
                amount,
            }
        }
    };
    let action = match event {
        CartEvent::LineVoided { .. } => "sale.line_voided",
        CartEvent::CartCancelled { .. } => "sale.cancelled",
        CartEvent::QtyReduced { .. } => "sale.qty_reduced",
    };
    let payload = json!({ "event": event, "approvals": ctx.approvers() });
    audit::append(tx, &ctx.actor().event(action, "cart", None, payload))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;
    use crate::sales::cart::CartLine;
    use crate::testkit::shop;

    #[test]
    fn hold_list_recall_with_a_price_notice() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let ctx = shop.till();
        let cart = Cart::new(Lang::Fr, vec![CartLine::product(&milk.id, Qty(2_000))]);
        let held = shop
            .db
            .write(move |tx| hold(tx, &ctx, " Table 4 ", cart))
            .unwrap();
        assert_eq!(
            (held.label.as_str(), held.cart.lines[0].expected_unit_price),
            ("Table 4", Some(Money(10_000)))
        );
        let terminal = shop.terminal.id.clone();
        assert_eq!(shop.db.read(move |c| list(c, &terminal)).unwrap().len(), 1);
        let (id, actor) = (milk.id.clone(), shop.owner.actor());
        shop.db
            .write(move |tx| {
                crate::catalog::products::set_price(
                    tx,
                    &actor,
                    &id,
                    crate::catalog::RETAIL_LEVEL_ID,
                    Money(11_000),
                )
            })
            .unwrap();
        let (ctx, id) = (shop.till(), held.id.clone());
        let recalled = shop.db.write(move |tx| recall(tx, &ctx, &id)).unwrap();
        assert_eq!(recalled.changed_lines, [1]);
        assert_eq!(recalled.priced.totals.total_ttc, Money(22_000));
        let terminal = shop.terminal.id.clone();
        assert!(
            shop.db
                .read(move |c| list(c, &terminal))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM stock_movements"),
            0,
            "holding never touches stock"
        );
    }

    #[test]
    fn another_terminal_cannot_take_the_cart_and_discard_is_journaled() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let ctx = shop.till();
        let cart = Cart::new(Lang::Fr, vec![CartLine::product(&milk.id, Qty(1_000))]);
        let held = shop
            .db
            .write(move |tx| hold(tx, &ctx, "Karim", cart))
            .unwrap();
        let mut elsewhere = shop.till();
        elsewhere.session.terminal_id = "other-terminal".into();
        let id = held.id.clone();
        let err = shop
            .db
            .write(move |tx| recall(tx, &elsewhere, &id))
            .unwrap_err();
        assert_eq!(err.code(), "held_cart_not_found");
        let (ctx, id) = (shop.till(), held.id.clone());
        shop.db.write(move |tx| discard(tx, &ctx, &id)).unwrap();
        let actions: String = shop
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT group_concat(action) FROM (SELECT action FROM audit_log WHERE action LIKE 'sale.%' ORDER BY seq)",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(actions, "sale.held,sale.held_discarded");
    }

    fn event_of(shop: &crate::testkit::Shop, action: &'static str) -> serde_json::Value {
        let payload: String = shop
            .db
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT payload FROM audit_log WHERE action = ?1",
                    [action],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        payload["event"].clone()
    }

    #[test]
    fn voids_and_cancellations_are_journaled_with_the_backends_figures() {
        // The till sends the cart on screen, never a figure: a stale preview cannot put another
        // line's name or amount in the journal (AUD-001, RPT-018).
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let oil = shop.product("Huile", Money(60_000), &shop.tva19);
        // The cashier may not give this discount: the voided line is still worth what it shows.
        let mut half = CartLine::product(&oil.id, Qty(1_000));
        half.discount = crate::tax::tva::Discount::Percent(crate::money::Rate(5_000));
        let cart = Cart::new(
            Lang::Fr,
            vec![CartLine::product(&milk.id, Qty(2_000)), half],
        );
        let ctx = shop.till();
        let (voided, cancelled) = (cart.clone(), cart);
        shop.db
            .write(move |tx| {
                record_cart_event(
                    tx,
                    &ctx,
                    CartEventInput::LineVoided {
                        cart: voided,
                        index: 1,
                    },
                )?;
                record_cart_event(tx, &ctx, CartEventInput::CartCancelled { cart: cancelled })
            })
            .unwrap();
        assert_eq!(
            event_of(&shop, "sale.line_voided"),
            json!({ "kind": "line_voided", "productId": oil.id, "description": "Huile", "qty": 1_000, "amount": 30_000 })
        );
        assert_eq!(
            event_of(&shop, "sale.cancelled"),
            json!({ "kind": "cart_cancelled", "lines": 2, "total": 50_000 })
        );
    }

    /// An open-price product rung from the grid without its price cannot be valued, but a cart the
    /// till can no longer price must stay voidable, cancellable and reducible (SAL-040/041,
    /// AUD-001): the journal keeps its name, its quantity and no amount.
    #[test]
    fn an_unpriced_open_price_line_does_not_block_a_void_or_a_cancellation() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let divers = shop.department_key("Divers");
        let cart = Cart::new(
            Lang::Fr,
            vec![
                CartLine::product(&milk.id, Qty(1_000)),
                CartLine::product(&divers, Qty(2_000)),
            ],
        );
        let ctx = shop.till();
        let (voided, lowered, cancelled) = (cart.clone(), cart.clone(), cart);
        shop.db
            .write(move |tx| {
                // the line has no price yet: the cart as a whole cannot be priced
                assert!(cart::value_cart(tx, &ctx, &voided).is_err());
                record_cart_event(
                    tx,
                    &ctx,
                    CartEventInput::LineVoided {
                        cart: voided,
                        index: 1,
                    },
                )?;
                record_cart_event(
                    tx,
                    &ctx,
                    CartEventInput::QtyReduced {
                        cart: lowered,
                        index: 1,
                        qty: Qty(1_000),
                    },
                )?;
                record_cart_event(tx, &ctx, CartEventInput::CartCancelled { cart: cancelled })
            })
            .unwrap();
        assert_eq!(
            event_of(&shop, "sale.line_voided"),
            json!({ "kind": "line_voided", "productId": divers, "description": "Divers", "qty": 2_000, "amount": 0 })
        );
        assert_eq!(
            event_of(&shop, "sale.qty_reduced"),
            json!({
                "kind": "qty_reduced", "productId": divers, "description": "Divers",
                "qtyBefore": 2_000, "qtyAfter": 1_000, "removedQty": 1_000, "amount": 0
            })
        );
        // the priced line still counts; the unpriced one is left out of the total
        assert_eq!(
            event_of(&shop, "sale.cancelled"),
            json!({ "kind": "cart_cancelled", "lines": 2, "total": 10_000 })
        );
    }

    /// Ten units rung and cut to one is a partial void: journaled with what left the cart.
    #[test]
    fn a_lowered_quantity_is_journaled_as_a_partial_void() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let cart = Cart::new(Lang::Fr, vec![CartLine::product(&milk.id, Qty(10_000))]);
        let ctx = shop.till();
        let lowered = cart.clone();
        shop.db
            .write(move |tx| {
                record_cart_event(
                    tx,
                    &ctx,
                    CartEventInput::QtyReduced {
                        cart: lowered,
                        index: 0,
                        qty: Qty(1_000),
                    },
                )
            })
            .unwrap();
        assert_eq!(
            event_of(&shop, "sale.qty_reduced"),
            json!({
                "kind": "qty_reduced", "productId": milk.id, "description": "Lait",
                "qtyBefore": 10_000, "qtyAfter": 1_000, "removedQty": 9_000, "amount": 90_000
            })
        );
        // Not a reduction: the same or a larger quantity, or none at all.
        for qty in [Qty(10_000), Qty(12_000), Qty(0)] {
            let (ctx, cart) = (shop.till(), cart.clone());
            let err = shop
                .db
                .write(move |tx| {
                    record_cart_event(
                        tx,
                        &ctx,
                        CartEventInput::QtyReduced {
                            cart,
                            index: 0,
                            qty,
                        },
                    )
                })
                .unwrap_err();
            assert_eq!(err.payload().field.as_deref(), Some("qty"), "{qty:?}");
        }
    }

    #[test]
    fn a_discarded_held_ticket_is_journaled_with_its_value() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let ctx = shop.till();
        let cart = Cart::new(Lang::Fr, vec![CartLine::product(&milk.id, Qty(3_000))]);
        let held = shop
            .db
            .write(move |tx| hold(tx, &ctx, "Karim", cart))
            .unwrap();
        let (ctx, id) = (shop.till(), held.id.clone());
        shop.db.write(move |tx| discard(tx, &ctx, &id)).unwrap();
        let payload: String = shop
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT payload FROM audit_log WHERE action = 'sale.held_discarded'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(
            payload,
            json!({ "label": "Karim", "lines": 1, "total": 30_000, "approvals": [] })
        );
    }

    #[test]
    fn a_void_names_a_line_of_the_cart() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let ctx = shop.till();
        let cart = Cart::new(Lang::Fr, vec![CartLine::product(&milk.id, Qty(1_000))]);
        let err = shop
            .db
            .write(move |tx| {
                record_cart_event(tx, &ctx, CartEventInput::LineVoided { cart, index: 1 })
            })
            .unwrap_err();
        assert_eq!(
            (err.code(), err.payload().field),
            ("value_invalid".into(), Some("index".into()))
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'sale.line_voided'"),
            0
        );
    }
}
