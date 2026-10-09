//! The sale transaction (spec §7.1; STK-001, PAY-004, TAX-006, DOC-005): inside one
//! `Db::write` it checks the open cash session, prices the cart, settles the tenders with the
//! stamp duty on the cash part, numbers and hashes the ticket, takes the stock (components for
//! a composite), records the drawer cash and appends one audit row. Any error rolls it all back.

use super::cart::{self, Cart};
use super::extensions::SaleExtras;
use crate::audit;
use crate::cash::{self, DrawerShare};
use crate::catalog::products::{self, ProductKind};
use crate::documents::{self, DocType, Document, DraftDocument, DraftLine, DraftPayment};
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::{Money, Qty, Rate};
use crate::payments::{self, PaymentInput, PaymentMethod, Settlement};
use crate::stock::{self, MovementReason, NewMovement, Shortage, StockNeed};
use crate::tax::tva::LineAmounts;
use crate::till::TillContext;
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaleInput {
    pub cart: Cart,
    /// Tenders: cash, card CIB, Edahabia, BaridiMob, cheque, transfer, and on the cart's
    /// customer's account credit (kridi) and vouchers (store credit).
    pub payments: Vec<PaymentInput>,
}

/// The validated ticket, how it was settled (duty, change) and the stock shortages to show
/// under the `warn` policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ValidatedSale {
    pub document: Document,
    pub settlement: Settlement,
    pub shortages: Vec<Shortage>,
    /// The customer's balance after this ticket (receipt, SAL-037), when there is a customer.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub customer_balance: Option<Money>,
}

impl ValidatedSale {
    /// The cash this ticket kept in the drawer that took it (a host service's answer: the till
    /// records it, [`cash::record_share`]).
    pub fn drawer_share(&self) -> Result<Option<DrawerShare>, AppError> {
        DrawerShare::of_settlement(&self.document.id, &self.settlement)
    }
}

/// Value of returned goods applied to the new sale of an exchange (RET-004).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoreCredit {
    pub amount: Money,
    pub return_id: String,
    pub return_number: String,
}

const ZERO_AMOUNTS: LineAmounts = LineAmounts {
    gross: Money::ZERO,
    discount_bp: Rate::ZERO,
    discount_amount: Money::ZERO,
    total_ht: Money::ZERO,
    tax: Money::ZERO,
    total_ttc: Money::ZERO,
};

/// Sells `input` (spec §7.1). Printing happens after the commit, in M1b.
pub fn create_sale(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: SaleInput,
) -> Result<ValidatedSale, AppError> {
    create_sale_with(tx, ctx, input, &SaleExtras::default())
}

/// A sale with the vertical packs' extras (register entries, serials, buyer…): the same
/// transaction, with `extensions::after_sale` before the audit row.
pub fn create_sale_with(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: SaleInput,
    extras: &SaleExtras,
) -> Result<ValidatedSale, AppError> {
    create_sale_with_credit(tx, ctx, input, None, extras)
}

/// Stock a ticket takes: product, quantity, unit cost (CMUP at the sale).
struct Take {
    product_id: String,
    qty: Qty,
    cost: Money,
}

pub(crate) fn create_sale_with_credit(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: SaleInput,
    credit: Option<&StoreCredit>,
    extras: &SaleExtras,
) -> Result<ValidatedSale, AppError> {
    sell(tx, ctx, input, credit, &[], extras)
}

/// Sells a cart some of whose line discounts were approved when they were given (the
/// restaurant's comps, RST-008): `granted[i]` is the reason code ticket line `i` keeps (its
/// hash covers it); pricing does not ask the cashier's discount right for those lines again
/// ([`cart::price_cart_granted`]). The till's pack extras ride along like a till sale's.
pub(crate) fn create_sale_granted(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: SaleInput,
    granted: &[Option<String>],
    extras: &SaleExtras,
) -> Result<ValidatedSale, AppError> {
    sell(tx, ctx, input, None, granted, extras)
}

fn sell(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: SaleInput,
    credit: Option<&StoreCredit>,
    granted: &[Option<String>],
    extras: &SaleExtras,
) -> Result<ValidatedSale, AppError> {
    // The till's own drawer, or the calling till's for a host service (Task 20 review).
    let session = cash::drawer(tx, ctx)?;
    // VRT-101: a van sale takes its goods from the van; every other sale from the default location.
    let location = match extras.location_id.as_deref() {
        Some(id) => crate::wholesale::rounds::van_location(tx, id)?,
        None => stock::default_location(tx)?,
    };
    let flags: Vec<bool> = granted.iter().map(Option::is_some).collect();
    let priced = cart::price_cart_granted(tx, ctx, &input.cart, &flags)?;
    // Before the settlement: a credit or voucher tender without a customer asks for the customer
    // first, whatever its amount (credit carries no duty, so a tender copied from the cash total
    // would otherwise be refused as exceeding the goods).
    let account = crate::customers::credit::check_account_tenders(
        tx,
        ctx,
        input.cart.customer_id.as_deref(),
        &input.payments,
    )?;
    let mut rules = cart::settle_rules(tx)?;
    rules.store_credit = credit.map_or(Money::ZERO, |c| c.amount);
    let settlement = payments::settle(&ctx.pack, priced.totals.total_ttc, &input.payments, rules)?;
    let max_change = Money(crate::settings::get::<i64>(tx, "sales.max_change")?);
    if max_change.is_positive() && settlement.change > max_change {
        return Err(AppError::validation("payments", "change_too_large"));
    }

    let mut lines: Vec<DraftLine> = Vec::with_capacity(priced.lines.len());
    let mut takes: Vec<Take> = Vec::new();
    // Audit references to the ticket lines. The ticket numbers its lines in draft order, where
    // a kit's component lines follow the kit, so the cart position would point to the wrong line.
    let mut overrides: Vec<serde_json::Value> = Vec::new();
    let mut open_items: Vec<serde_json::Value> = Vec::new();
    let mut discounts: Vec<serde_json::Value> = Vec::new();
    // Lines discounted by a promotion or the customer's standing discount (`sale_line_promotions`).
    let mut promo_lines: Vec<(String, crate::promotions::AutoDiscount)> = Vec::new();
    // The ticket line of each cart line, for the packs' part of the sale.
    let mut main_line_ids: Vec<String> = Vec::with_capacity(priced.lines.len());
    for (i, p) in priced.lines.iter().enumerate() {
        let line_id = new_id();
        main_line_ids.push(line_id.clone());
        let line_no = lines.len() + 1;
        let reason_code = granted.get(i).cloned().flatten();
        if p.price_overridden {
            overrides.push(json!({ "lineNo": line_no, "lineId": line_id }));
        }
        if p.product_id.is_none() {
            open_items.push(json!({ "lineNo": line_no, "lineId": line_id }));
        }
        if p.amounts.discount_amount.is_positive() {
            let mut discount = json!({
                "lineNo": line_no,
                "lineId": line_id,
                "amount": p.amounts.discount_amount,
            });
            if let Some(auto) = &p.auto_discount {
                discount["source"] = json!(auto.source);
                discount["promotionId"] = json!(auto.promotion_id);
            }
            if let Some(reason) = &reason_code {
                discount["reason"] = json!(reason);
            }
            discounts.push(discount);
        }
        lines.push(DraftLine {
            id: line_id.clone(),
            product_id: p.product_id.clone(),
            description: p.description.clone(),
            qty: p.qty,
            unit_code: p.unit_code.clone(),
            unit_price: p.unit_price,
            price_overridden: p.price_overridden,
            tax_rate_bp: p.tax_rate_bp,
            amounts: p.amounts,
            cost_price: p.cost_price,
            parent_line_id: None,
            origin_line_id: None,
            restock_location_id: None,
            reason_code,
            note: p.note.clone(),
        });
        if let Some(auto) = &p.auto_discount {
            promo_lines.push((line_id.clone(), auto.clone()));
        }
        let Some(product_id) = &p.product_id else {
            continue;
        };
        let product = products::get(tx, product_id)?;
        for (component, qty) in stock::expand(tx, &product, p.qty)? {
            if product.kind == ProductKind::Composite {
                lines.push(DraftLine {
                    id: new_id(),
                    product_id: Some(component.id.clone()),
                    description: component.name_in(priced.lang).to_owned(),
                    qty,
                    unit_code: component.unit_code.clone(),
                    unit_price: Money::ZERO,
                    price_overridden: false,
                    tax_rate_bp: p.tax_rate_bp,
                    amounts: ZERO_AMOUNTS,
                    cost_price: component.cost_price,
                    parent_line_id: Some(line_id.clone()),
                    origin_line_id: None,
                    restock_location_id: None,
                    reason_code: None,
                    note: None,
                });
            }
            if component.track_stock {
                takes.push(Take {
                    product_id: component.id,
                    qty,
                    cost: component.cost_price,
                });
            }
        }
    }
    let needs: Vec<StockNeed> = takes
        .iter()
        .map(|t| StockNeed {
            product_id: t.product_id.clone(),
            location_id: location.id.clone(),
            qty: t.qty,
        })
        .collect();
    let shortages = stock::check_available(tx, &needs)?;

    let mut draft_payments: Vec<DraftPayment> = Vec::new();
    if let Some(c) = credit {
        draft_payments.push(DraftPayment {
            method: PaymentMethod::Voucher,
            amount: c.amount,
            reference: Some(c.return_number.clone()),
            cheque_bank: None,
            cheque_date: None,
        });
    }
    draft_payments.extend(settlement.tenders.iter().map(|t| DraftPayment {
        method: t.method,
        amount: t.amount,
        reference: t.reference.clone(),
        cheque_bank: t.cheque_bank.clone(),
        cheque_date: t.cheque_date.clone(),
    }));
    if settlement.change.is_positive() {
        draft_payments.push(DraftPayment {
            method: PaymentMethod::Change,
            amount: settlement.change.checked_neg().unwrap_or(Money::ZERO),
            reference: None,
            cheque_bank: None,
            cheque_date: None,
        });
    }
    let note = crate::catalog::clean_opt(input.cart.note.as_deref(), "note", 200)?;
    let draft = DraftDocument {
        doc_type: DocType::Ticket,
        terminal_code: ctx.session.terminal_code.clone(),
        business_date: ctx.business_date(),
        customer_id: input.cart.customer_id.clone(),
        supplier_id: None,
        user_id: ctx.session.user_id.clone(),
        terminal_id: ctx.session.terminal_id.clone(),
        cash_session_id: Some(session.id.clone()),
        price_level_id: Some(priced.price_level_id.clone()),
        lang: priced.lang,
        tax_regime: priced.tax_regime,
        price_mode: priced.price_mode,
        stamp_mode: settlement.stamp_mode,
        stamp_duty: settlement.stamp_duty,
        stamp_charged: settlement.stamp_charged,
        rounding: settlement.rounding,
        paid_total: settlement.paid_total,
        change_given: settlement.change,
        credit_amount: account.credit,
        note,
        parent_id: credit.map(|c| c.return_id.clone()),
        source_ref: None,
        lines,
        payments: draft_payments,
    };
    let id = documents::insert_draft(tx, &draft, ctx.now)?;
    let document = documents::validate(tx, &id, ctx.now)?;
    crate::promotions::record_sale(tx, &document.id, &promo_lines)?;

    let actor = ctx.actor();
    let mut expired_lots = 0_usize;
    for take in &takes {
        let movement = NewMovement {
            product_id: take.product_id.clone(),
            location_id: location.id.clone(),
            qty_delta: take
                .qty
                .checked_neg()
                .ok_or_else(|| AppError::validation("qty", "qty_invalid"))?,
            reason: MovementReason::Sale,
            reason_code: None,
            ref_type: Some("document".into()),
            ref_id: Some(document.id.clone()),
            unit_cost: Some(take.cost),
            note: None,
        };
        let taken = crate::stock::lots::take(tx, &actor, &movement, &ctx.business_day(), ctx.now)?;
        expired_lots += taken.iter().filter(|t| t.expired).count();
    }
    if let Some(share) = DrawerShare::of_settlement(&document.id, &settlement)? {
        cash::book(tx, ctx, &session.id, &share)?;
    }
    let customer_balance = crate::customers::credit::book_sale(tx, ctx, &account, &document.id)?;
    super::extensions::after_sale(tx, ctx, &input, extras, &document, &main_line_ids)?;
    audit::append(
        tx,
        &actor.event(
            "sale.created",
            "document",
            Some(&document.id),
            json!({
                "number": document.number,
                "totalDue": document.total_due,
                "stampDuty": document.stamp_duty,
                "lines": priced.lines.len(),
                "priceOverrides": overrides,
                "openItems": open_items,
                "discounts": discounts,
                "approvals": ctx.approvers(),
                "storeCredit": credit.map(|c| c.amount),
                "shortages": shortages.len(),
                "expiredLots": expired_lots,
                "customerId": input.cart.customer_id,
                "credit": account.credit,
                "voucher": account.voucher,
            }),
        ),
    )?;
    Ok(ValidatedSale {
        document,
        settlement,
        shortages,
        customer_balance,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Permission;
    use crate::auth::session::OverrideGrant;
    use crate::catalog::UNIT_PIECE_ID;
    use crate::catalog::products::{Component, ProductInput};
    use crate::company::{PriceMode, TaxRegime};
    use crate::documents::DocStatus;
    use crate::lang::Lang;
    use crate::sales::cart::CartLine;
    use crate::tax::timbre::StampMode;
    use crate::tax::tva::Discount;
    use crate::testkit::{NOON, Shop, shop, shop_with};
    use serde_json::json;

    fn sell(
        shop: &Shop,
        now: i64,
        lines: Vec<CartLine>,
        payments: Vec<PaymentInput>,
    ) -> Result<ValidatedSale, AppError> {
        let ctx = TillContext::new(shop.cashier.clone(), shop.pack.clone(), now);
        let input = SaleInput {
            cart: Cart::new(Lang::Fr, lines),
            payments,
        };
        shop.db.write(move |tx| create_sale(tx, &ctx, input))
    }

    #[test]
    fn a_cash_sale_is_one_consistent_transaction() {
        let shop = shop();
        let milk = shop.product("Lait", Money(45_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(100_000);
        let sale = sell(
            &shop,
            NOON,
            vec![CartLine::product(&milk.id, Qty(1_000))],
            vec![PaymentInput::cash(Money(100_000))],
        )
        .unwrap();
        let doc = &sale.document;
        assert_eq!(
            (doc.number.as_deref(), doc.status),
            (Some("TK2026-C1-000001"), DocStatus::Validated)
        );
        assert_eq!(
            (doc.total_ttc, doc.stamp_duty, doc.total_due),
            (Money(45_000), Money(500), Money(45_500))
        );
        assert_eq!(
            (doc.paid_total, doc.change_given),
            (Money(100_000), Money(54_500))
        );
        let tenders: Vec<(PaymentMethod, Money)> =
            doc.payments.iter().map(|p| (p.method, p.amount)).collect();
        assert_eq!(
            tenders,
            [
                (PaymentMethod::Cash, Money(100_000)),
                (PaymentMethod::Change, Money(-54_500))
            ]
        );
        let id = milk.id.clone();
        assert_eq!(
            shop.db.read(move |c| stock::on_hand(c, &id)).unwrap(),
            Qty(9_000)
        );
        let terminal = shop.terminal.id.clone();
        let session = shop
            .db
            .read(move |c| cash::require_open(c, &terminal))
            .unwrap();
        let sid = session.id.clone();
        assert_eq!(
            shop.db.read(move |c| cash::expected_cash(c, &sid)).unwrap(),
            Money(145_500)
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'sale.created'"),
            1
        );
        let chain = shop
            .db
            .read(|c| documents::verify_chain(c, DocType::Ticket, "C1"))
            .unwrap();
        assert_eq!((chain.rows, chain.first_broken_seq), (1, None));
        assert_eq!(doc.lines[0].cost_price, Money::ZERO);
    }

    #[test]
    fn nothing_is_sold_without_an_open_session_or_enough_money() {
        let shop = shop();
        let milk = shop.product("Lait", Money(45_000), &shop.tva19);
        let err = sell(
            &shop,
            NOON,
            vec![CartLine::product(&milk.id, Qty(1_000))],
            vec![PaymentInput::cash(Money(45_500))],
        )
        .unwrap_err();
        assert_eq!(err.code(), "cash_session_closed");
        shop.open_till(0);
        let err = sell(
            &shop,
            NOON,
            vec![CartLine::product(&milk.id, Qty(1_000))],
            vec![PaymentInput::cash(Money(45_000))],
        )
        .unwrap_err();
        assert_eq!(
            err.code(),
            "payment_insufficient",
            "the 5 DA duty is due too"
        );
        let credit = PaymentInput {
            method: PaymentMethod::Credit,
            ..PaymentInput::cash(Money(45_500))
        };
        let err = sell(
            &shop,
            NOON,
            vec![CartLine::product(&milk.id, Qty(1_000))],
            vec![credit],
        )
        .unwrap_err();
        assert_eq!(err, AppError::validation("customerId", "customer_required"));
        assert_eq!(shop.count("SELECT COUNT(*) FROM documents"), 0);
        assert_eq!(shop.count("SELECT COUNT(*) FROM counters"), 0);
        assert_eq!(shop.count("SELECT COUNT(*) FROM stock_movements"), 0);
    }

    #[test]
    fn a_card_and_cash_split_pays_duty_only_on_the_cash_part() {
        let shop = shop();
        let tv = shop.product("Téléviseur", Money(5_000_000), &shop.tva19);
        shop.open_till(0);
        let card =
            PaymentInput::electronic(PaymentMethod::CardCib, Money(4_000_000), Some("AUT-7781"));
        let sale = sell(
            &shop,
            NOON,
            vec![CartLine::product(&tv.id, Qty(1_000))],
            vec![card, PaymentInput::cash(Money(1_010_000))],
        )
        .unwrap();
        // Cash part 10 000 DA → 100 tranches × 1 DA.
        assert_eq!(
            (sale.document.stamp_duty, sale.document.total_due),
            (Money(10_000), Money(5_010_000))
        );
        assert_eq!(
            sale.document.payments[0].reference.as_deref(),
            Some("AUT-7781")
        );
        assert_eq!(sale.settlement.change, Money::ZERO);
        let terminal = shop.terminal.id.clone();
        let sid = shop
            .db
            .read(move |c| cash::require_open(c, &terminal))
            .unwrap()
            .id;
        assert_eq!(
            shop.db.read(move |c| cash::expected_cash(c, &sid)).unwrap(),
            Money(1_010_000)
        );
    }

    #[test]
    fn a_kit_takes_its_components_and_shows_them_as_lines() {
        let shop = shop();
        let pen = shop.product("Stylo", Money(5_000), &shop.tva19);
        let book = shop.product("Cahier", Money(8_000), &shop.tva19);
        shop.stock_in(&pen.id, 10_000);
        shop.stock_in(&book.id, 10_000);
        let mut kit =
            ProductInput::simple("Kit scolaire", UNIT_PIECE_ID, &shop.tva19, Money(20_000));
        kit.kind = ProductKind::Composite;
        kit.components = vec![
            Component {
                component_id: pen.id.clone(),
                qty: Qty(2_000),
            },
            Component {
                component_id: book.id.clone(),
                qty: Qty(3_000),
            },
        ];
        let kit = shop.product_with(kit);
        shop.open_till(0);
        let sale = sell(
            &shop,
            NOON,
            vec![CartLine::product(&kit.id, Qty(2_000))],
            vec![PaymentInput::cash(Money(40_500))],
        )
        .unwrap();
        let lines: Vec<(&str, Qty, Money, bool)> = sale
            .document
            .lines
            .iter()
            .map(|l| {
                (
                    l.description.as_str(),
                    l.qty,
                    l.total_ttc,
                    l.parent_line_id.is_some(),
                )
            })
            .collect();
        assert_eq!(
            lines,
            [
                ("Kit scolaire", Qty(2_000), Money(40_000), false),
                ("Stylo", Qty(4_000), Money::ZERO, true),
                ("Cahier", Qty(6_000), Money::ZERO, true)
            ]
        );
        let (p, b) = (pen.id.clone(), book.id.clone());
        let levels = shop
            .db
            .read(move |c| Ok((stock::on_hand(c, &p)?, stock::on_hand(c, &b)?)))
            .unwrap();
        assert_eq!(levels, (Qty(6_000), Qty(4_000)));
    }

    #[test]
    fn the_audit_row_points_to_the_ticket_lines_after_a_kit() {
        let shop = shop();
        let pen = shop.product("Stylo", Money(5_000), &shop.tva19);
        let book = shop.product("Cahier", Money(8_000), &shop.tva19);
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let bread = shop.product("Pain", Money(5_000), &shop.tva19);
        let mut kit =
            ProductInput::simple("Kit scolaire", UNIT_PIECE_ID, &shop.tva19, Money(20_000));
        kit.kind = ProductKind::Composite;
        kit.components = vec![
            Component {
                component_id: pen.id.clone(),
                qty: Qty(1_000),
            },
            Component {
                component_id: book.id.clone(),
                qty: Qty(1_000),
            },
        ];
        let kit = shop.product_with(kit);
        shop.open_till(0);
        let grant = |permission| OverrideGrant {
            approver_id: shop.owner.user_id.clone(),
            approver_name: "Amine".into(),
            permission,
            granted_at: NOON,
            discount_limit_bp: 10_000,
        };
        let ctx = TillContext::new(shop.cashier.clone(), shop.pack.clone(), NOON)
            .with_approval(grant(Permission::SalesDiscount))
            .with_approval(grant(Permission::SalesPriceOverride));
        let mut discounted = CartLine::product(&milk.id, Qty(1_000));
        discounted.discount = Discount::Amount(Money(1_000));
        let overridden = CartLine {
            unit_price: Some(Money(4_000)),
            ..CartLine::product(&bread.id, Qty(1_000))
        };
        let open = CartLine::open_item("Service", Qty(1_000), Money(3_000), &shop.tva19);
        let input = SaleInput {
            cart: Cart::new(
                Lang::Fr,
                vec![
                    CartLine::product(&kit.id, Qty(1_000)),
                    discounted,
                    overridden,
                    open,
                ],
            ),
            payments: vec![PaymentInput::cash(Money(40_000))],
        };
        let sale = shop
            .db
            .write(move |tx| create_sale(tx, &ctx, input))
            .unwrap();
        // Ticket: 1 kit, 2 Stylo, 3 Cahier (components), 4 Lait, 5 Pain, 6 Service.
        let line = |description: &str| {
            let l = sale
                .document
                .lines
                .iter()
                .find(|l| l.description == description)
                .unwrap();
            (l.line_no, l.id.clone())
        };
        let (milk_no, milk_id) = line("Lait");
        let (bread_no, bread_id) = line("Pain");
        let (open_no, open_id) = line("Service");
        assert_eq!((milk_no, bread_no, open_no), (4, 5, 6));
        let doc_id = sale.document.id.clone();
        let payload: serde_json::Value = shop
            .db
            .read(move |c| {
                let text: String = c.query_row(
                    "SELECT payload FROM audit_log WHERE action = 'sale.created' AND entity_id = ?1",
                    [&doc_id],
                    |r| r.get(0),
                )?;
                Ok(serde_json::from_str(&text).unwrap())
            })
            .unwrap();
        assert_eq!(
            payload["discounts"],
            json!([{ "lineNo": milk_no, "lineId": milk_id, "amount": 1_000 }])
        );
        assert_eq!(
            payload["priceOverrides"],
            json!([{ "lineNo": bread_no, "lineId": bread_id }])
        );
        assert_eq!(
            payload["openItems"],
            json!([{ "lineNo": open_no, "lineId": open_id }])
        );
        let owner = shop.owner.user_id.as_str();
        assert_eq!(
            payload["approvals"],
            json!([["sales.discount", owner], ["sales.price_override", owner]])
        );
    }

    #[test]
    fn stock_shortages_warn_by_default_and_block_on_request() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.open_till(0);
        let sale = sell(
            &shop,
            NOON,
            vec![CartLine::product(&milk.id, Qty(2_000))],
            vec![PaymentInput::cash(Money(20_000))],
        )
        .unwrap();
        assert_eq!(
            (sale.shortages.len(), sale.shortages[0].needed),
            (1, Qty(2_000))
        );
        let id = milk.id.clone();
        assert_eq!(
            shop.db.read(move |c| stock::on_hand(c, &id)).unwrap(),
            Qty(-2_000)
        );
        shop.set("stock.negative_policy", json!("block"));
        let err = sell(
            &shop,
            NOON,
            vec![CartLine::product(&milk.id, Qty(1_000))],
            vec![PaymentInput::cash(Money(10_000))],
        )
        .unwrap_err();
        assert_eq!(err.code(), "stock_insufficient");
        assert_eq!(
            shop.count("SELECT last_number FROM counters"),
            1,
            "the refused sale took no number"
        );
    }

    #[test]
    fn duty_modes_regimes_and_price_modes_are_recorded() {
        let shop = shop_with(TaxRegime::Ifu, PriceMode::Ht);
        let item = shop.product("Ciment", Money(100_000), &shop.tva19);
        shop.set("stamp_duty.mode", json!("absorb"));
        shop.open_till(0);
        let sale = sell(
            &shop,
            NOON,
            vec![CartLine::product(&item.id, Qty(1_000))],
            vec![PaymentInput::cash(Money(119_000))],
        )
        .unwrap();
        let doc = sale.document;
        assert_eq!(
            (doc.tax_regime, doc.price_mode, doc.stamp_mode),
            (TaxRegime::Ifu, PriceMode::Ht, StampMode::Absorb)
        );
        assert_eq!(
            (doc.subtotal_ht, doc.tax_total, doc.total_ttc),
            (Money(100_000), Money(19_000), Money(119_000))
        );
        assert_eq!(
            (doc.stamp_duty, doc.total_due),
            (Money(1_200), Money(119_000)),
            "absorbed: recorded, not charged"
        );
    }

    #[test]
    fn create_sale_is_create_sale_with_no_extras() {
        let shop = shop();
        let milk = shop.product("Lait", Money(45_000), &shop.tva19);
        shop.open_till(0);
        let ctx = TillContext::new(shop.cashier.clone(), shop.pack.clone(), NOON);
        let input = SaleInput {
            cart: Cart::new(Lang::Fr, vec![CartLine::product(&milk.id, Qty(1_000))]),
            payments: vec![PaymentInput::cash(Money(45_500))],
        };
        let sale = shop
            .db
            .write(move |tx| {
                create_sale_with(tx, &ctx, input, &crate::sales::SaleExtras::default())
            })
            .unwrap();
        assert_eq!(sale.document.number.as_deref(), Some("TK2026-C1-000001"));
    }

    #[test]
    fn numbering_restarts_on_the_first_business_day_of_the_year() {
        let shop = shop();
        let bread = shop.product("Pain", Money(1_000), &shop.exempt);
        shop.open_till(0);
        // 2026-12-31 22:59 UTC = 23:59 in Algiers; one minute later it is 2027 there.
        let before = sell(
            &shop,
            1_798_757_940_000,
            vec![CartLine::product(&bread.id, Qty(1_000))],
            vec![PaymentInput::cash(Money(1_000))],
        )
        .unwrap();
        let after = sell(
            &shop,
            1_798_758_000_000,
            vec![CartLine::product(&bread.id, Qty(1_000))],
            vec![PaymentInput::cash(Money(1_000))],
        )
        .unwrap();
        assert_eq!(before.document.number.as_deref(), Some("TK2026-C1-000001"));
        assert_eq!(
            (
                after.document.number.as_deref(),
                after.document.business_date.as_str()
            ),
            (Some("TK2027-C1-000001"), "2027-01-01")
        );
        assert_eq!(
            after.document.prev_hash, before.document.hash,
            "one chain across years"
        );
    }
}
