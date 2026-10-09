//! Drafts of invoices, quotes, proformas and delivery notes (the editor; SAL-046 "cart →
//! document"): typed as a cart, priced by `pricing`, written as a draft document with its cart
//! (the editor reopens exactly what was typed), edited in place, cancelled, never numbered
//! before validation.

use super::mentions::{self, MentionCheck};
use super::pricing::{self, PricedDocument};
use crate::audit;
use crate::catalog::products::{self, ProductKind};
use crate::catalog::{clean_opt, tax_rates};
use crate::company::PriceMode;
use crate::documents::links::{self, LinkKind};
use crate::documents::{
    self, DocOrigin, DocStatus, DocType, Document, DraftDocument, DraftExtras, DraftLine,
    DraftPayment,
};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::{BP_SCALE, Money, Rate};
use crate::sales::cart::{self, Cart, CartLine};
use crate::stock;
use crate::tax::timbre::StampMode;
use crate::tax::tva::{Discount, LineAmounts};
use crate::till::TillContext;
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

/// The kinds typed in the editor (credit notes are made from an invoice, never typed). A customer
/// order is drafted here (ORD-002) and validated by `orders::create`, never by `validate`.
pub const EDITOR_KINDS: [DocType; 5] = [
    DocType::Invoice,
    DocType::Quote,
    DocType::Proforma,
    DocType::DeliveryNote,
    DocType::CustomerOrder,
];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DraftInput {
    pub doc_type: DocType,
    pub cart: Cart,
    #[serde(default)]
    #[specta(optional)]
    pub valid_until: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub due_date: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub second_lang: Option<Lang>,
    /// The saved draft being edited, for the preview (a draft converted from a quote or
    /// proforma keeps its offered prices). Saving takes the draft from its own `id`.
    #[serde(default)]
    #[specta(optional)]
    pub draft_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DraftPreview {
    pub priced: PricedDocument,
    pub mentions: MentionCheck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DraftCart {
    pub id: String,
    pub doc_type: DocType,
    pub cart: Cart,
    pub valid_until: Option<String>,
    pub due_date: Option<String>,
    pub second_lang: Option<Lang>,
    pub origin: Option<DocOrigin>,
}

const ZERO_AMOUNTS: LineAmounts = LineAmounts {
    gross: Money::ZERO,
    discount_bp: Rate::ZERO,
    discount_amount: Money::ZERO,
    total_ht: Money::ZERO,
    tax: Money::ZERO,
    total_ttc: Money::ZERO,
};

fn too_large() -> AppError {
    AppError::validation("amount", "amount_invalid")
}

/// Today plus `days`, `YYYY-MM-DD` (quote validity, invoice due date).
pub fn add_days(ctx: &TillContext, days: i64) -> Result<String, AppError> {
    let days = u64::try_from(days).map_err(|_| AppError::validation("days", "value_invalid"))?;
    Ok(ctx
        .business_date()
        .checked_add_days(chrono::Days::new(days))
        .ok_or_else(|| AppError::validation("days", "value_invalid"))?
        .format("%Y-%m-%d")
        .to_string())
}

/// A day is `YYYY-MM-DD` exactly (chrono also reads `2026-10-5`, which the column refuses) and
/// not before today.
pub(crate) fn check_day(field: &str, day: &Option<String>, today: &str) -> Result<(), AppError> {
    let exact = |d: &str| {
        NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .is_ok_and(|date| date.format("%Y-%m-%d").to_string() == d)
    };
    if let Some(d) = day
        && (!exact(d) || d.as_str() < today)
    {
        return Err(AppError::validation(field, "date_invalid"));
    }
    Ok(())
}

fn check_input(ctx: &TillContext, input: &DraftInput) -> Result<(), AppError> {
    if !EDITOR_KINDS.contains(&input.doc_type) {
        return Err(AppError::validation("docType", "document_kind_invalid"));
    }
    if matches!(
        input.doc_type,
        DocType::Invoice | DocType::DeliveryNote | DocType::CustomerOrder
    ) && input.cart.customer_id.is_none()
    {
        return Err(AppError::validation("customerId", "customer_required"));
    }
    if input.valid_until.is_some() && !matches!(input.doc_type, DocType::Quote | DocType::Proforma)
    {
        return Err(AppError::validation("validUntil", "value_invalid"));
    }
    if input.due_date.is_some() && input.doc_type != DocType::Invoice {
        return Err(AppError::validation("dueDate", "value_invalid"));
    }
    let today = ctx.business_day();
    check_day("validUntil", &input.valid_until, &today)?;
    check_day("dueDate", &input.due_date, &today)?;
    if input.second_lang == Some(input.cart.lang) {
        return Err(AppError::validation("secondLang", "value_invalid"));
    }
    Ok(())
}

/// The editor's live figures (the UI never computes one) and the mentions still missing.
pub fn preview(
    conn: &Connection,
    ctx: &TillContext,
    input: &DraftInput,
) -> Result<DraftPreview, AppError> {
    check_input(ctx, input)?;
    // A draft validated meanwhile keeps no offer: its lines are priced as typed.
    let offered = match &input.draft_id {
        Some(id) => {
            let doc = documents::get(conn, id)?;
            if doc.status == DocStatus::Draft {
                offer_terms(conn, &doc)?
            } else {
                Vec::new()
            }
        }
        None => Vec::new(),
    };
    Ok(DraftPreview {
        priced: pricing::price_offer(conn, ctx, input.doc_type, &input.cart, &offered)?,
        mentions: mentions::check(conn, input.cart.customer_id.as_deref())?,
    })
}

/// The offered lines a draft converted from a validated quote or proforma keeps (ORD-001: "at
/// the offered prices"), none for any other draft.
fn offer_terms(conn: &Connection, draft: &Document) -> Result<Vec<CartLine>, AppError> {
    if !matches!(draft.origin, Some(DocOrigin::Quote | DocOrigin::Proforma)) {
        return Ok(Vec::new());
    }
    let source = links::sources(conn, &draft.id)?.into_iter().find(|l| {
        l.link.kind == LinkKind::Converted
            && l.status == DocStatus::Validated
            && matches!(l.doc_type, DocType::Quote | DocType::Proforma)
    });
    match source {
        Some(link) => Ok(offered_cart(conn, &documents::get(conn, &link.link.from_id)?)?.lines),
        None => Ok(Vec::new()),
    }
}

/// The draft document of a priced cart: one line per cart line and, for a kit, its component
/// lines at zero amounts (they carry the stock, like the sale).
pub(crate) fn draft_document(
    conn: &Connection,
    ctx: &TillContext,
    doc_type: DocType,
    cart: &Cart,
    priced: &PricedDocument,
) -> Result<DraftDocument, AppError> {
    let mut lines: Vec<DraftLine> = Vec::with_capacity(priced.cart.lines.len());
    for p in &priced.cart.lines {
        let line_id = new_id();
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
            reason_code: None,
            note: p.note.clone(),
        });
        let Some(product_id) = &p.product_id else {
            continue;
        };
        let product = products::get(conn, product_id)?;
        if product.kind != ProductKind::Composite {
            continue;
        }
        for (component, qty) in stock::expand(conn, &product, p.qty)? {
            lines.push(DraftLine {
                id: new_id(),
                product_id: Some(component.id.clone()),
                description: component.name_in(priced.cart.lang).to_owned(),
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
    }
    Ok(DraftDocument {
        doc_type,
        terminal_code: ctx.session.terminal_code.clone(),
        business_date: ctx.business_date(),
        customer_id: cart.customer_id.clone(),
        supplier_id: None,
        user_id: ctx.session.user_id.clone(),
        terminal_id: ctx.session.terminal_id.clone(),
        cash_session_id: None,
        price_level_id: Some(priced.cart.price_level_id.clone()),
        lang: priced.cart.lang,
        tax_regime: priced.cart.tax_regime,
        price_mode: priced.cart.price_mode,
        stamp_mode: cart::settle_rules(conn)?.stamp_mode,
        stamp_duty: Money::ZERO,
        stamp_charged: Money::ZERO,
        rounding: Money::ZERO,
        paid_total: Money::ZERO,
        change_given: Money::ZERO,
        credit_amount: Money::ZERO,
        note: clean_opt(cart.note.as_deref(), "note", 200)?,
        parent_id: None,
        source_ref: None,
        lines,
        payments: Vec::new(),
    })
}

/// Creates (`id` = `None`) or rewrites a draft; returns it as stored.
pub fn save_draft(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: Option<&str>,
    input: DraftInput,
) -> Result<Document, AppError> {
    check_input(ctx, &input)?;
    let existing = match id {
        Some(id) => {
            let doc = documents::get(tx, id)?;
            if doc.status != DocStatus::Draft {
                return Err(AppError::conflict("document_already_validated"));
            }
            if doc.doc_type != input.doc_type {
                return Err(AppError::validation("docType", "document_kind_invalid"));
            }
            Some(doc)
        }
        None => None,
    };
    let offered = match &existing {
        Some(doc) => offer_terms(tx, doc)?,
        None => Vec::new(),
    };
    let priced = pricing::price_offer(tx, ctx, input.doc_type, &input.cart, &offered)?;
    let draft = draft_document(tx, ctx, input.doc_type, &input.cart, &priced)?;
    let doc_id = match &existing {
        Some(doc) => {
            documents::replace_draft(tx, &doc.id, &draft, ctx.now)?;
            doc.id.clone()
        }
        None => documents::insert_draft(tx, &draft, ctx.now)?,
    };
    let origin = existing
        .as_ref()
        .and_then(|d| d.origin)
        .unwrap_or(DocOrigin::Direct);
    documents::set_extras(
        tx,
        &doc_id,
        &DraftExtras {
            valid_until: input.valid_until.clone(),
            due_date: input.due_date.clone(),
            second_lang: input.second_lang,
            credit_kind: None,
            origin: Some(origin),
        },
    )?;
    documents::store_cart(tx, &doc_id, &serde_json::to_value(&input.cart)?, ctx.now)?;
    let doc = documents::get(tx, &doc_id)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "document.draft_saved",
            "document",
            Some(&doc_id),
            json!({
                "docType": doc.doc_type,
                "lines": priced.cart.lines.len(),
                "totalTtc": doc.total_ttc,
                "customerId": doc.customer_id,
                "approvals": ctx.approvers(),
            }),
        ),
    )?;
    Ok(doc)
}

/// What the editor reopens: the stored cart, else the lines rebuilt (a converted draft).
pub fn draft_cart(conn: &Connection, id: &str) -> Result<DraftCart, AppError> {
    let doc = documents::get(conn, id)?;
    if doc.status != DocStatus::Draft {
        return Err(AppError::conflict("document_already_validated"));
    }
    let cart = match documents::stored_cart(conn, id)? {
        Some(value) => serde_json::from_value(value)?,
        None => cart_from_lines(conn, &doc)?,
    };
    Ok(DraftCart {
        id: doc.id,
        doc_type: doc.doc_type,
        cart,
        valid_until: doc.valid_until,
        due_date: doc.due_date,
        second_lang: doc.second_lang,
        origin: doc.origin,
    })
}

/// A cart from stored lines: catalogue products at their catalogue price with their discount,
/// an overridden price or an open-price product (SAL-019: a department key, a repair) at its
/// stored price, open items at their price with the active rate of the same percentage.
pub(crate) fn cart_from_lines(conn: &Connection, doc: &Document) -> Result<Cart, AppError> {
    let mut lines = Vec::new();
    for l in doc.lines.iter().filter(|l| l.parent_line_id.is_none()) {
        let discount = if l.discount_bp.0 > 0 {
            Discount::Percent(l.discount_bp)
        } else if l.discount_amount.is_positive() {
            Discount::Amount(l.discount_amount)
        } else {
            Discount::None
        };
        let line = match &l.product_id {
            Some(product_id) => CartLine {
                product_id: Some(product_id.clone()),
                description: None,
                qty: l.qty,
                unit_price: (l.price_overridden
                    || crate::catalog::departments::is_open_price(conn, product_id)?)
                .then_some(l.unit_price),
                discount,
                tax_rate_id: None,
                note: l.note.clone(),
                expected_unit_price: Some(l.unit_price),
                separate: false,
            },
            None => {
                let rate_id: Option<String> = conn
                    .query_row(
                        "SELECT id FROM tax_rates WHERE rate_bp = ?1 AND active = 1 ORDER BY code LIMIT 1",
                        [l.tax_rate_bp],
                        |r| r.get(0),
                    )
                    .optional()?;
                CartLine {
                    product_id: None,
                    description: Some(l.description.clone()),
                    qty: l.qty,
                    unit_price: Some(l.unit_price),
                    discount,
                    tax_rate_id: Some(
                        rate_id.ok_or_else(|| AppError::conflict("tax_rate_not_found"))?,
                    ),
                    note: l.note.clone(),
                    expected_unit_price: None,
                    separate: false,
                }
            }
        };
        lines.push(line);
    }
    Ok(Cart {
        lines,
        price_level_id: doc.price_level_id.clone(),
        customer_id: doc.customer_id.clone(),
        lang: doc.lang,
        note: doc.note.clone(),
    })
}

/// The cart of a validated quote or proforma at its offered prices (a conversion that is not
/// re-priced): [`cart_from_lines`] with every catalogue line at the price it was offered at,
/// expected at that price. An exempt buyer's line of a TTC shop holds its HT price at 0 %: its
/// price and fixed discount are typed back TTC at the product's rate, which pricing takes off
/// again exactly (the round trip of a rate above zero is exact in centimes).
pub(crate) fn offered_cart(conn: &Connection, doc: &Document) -> Result<Cart, AppError> {
    let mut cart = cart_from_lines(conn, doc)?;
    let tops = doc.lines.iter().filter(|l| l.parent_line_id.is_none());
    for (line, stored) in cart.lines.iter_mut().zip(tops) {
        let Some(product_id) = &line.product_id else {
            continue;
        };
        let mut price = stored.unit_price;
        if doc.price_mode == PriceMode::Ttc && stored.tax_rate_bp == Rate::ZERO {
            let product = products::get(conn, product_id)?;
            let rate = i64::from(tax_rates::get(conn, &product.tax_rate_id)?.rate_bp.0);
            let ttc = |m: Money| m.mul_div(BP_SCALE + rate, BP_SCALE).ok_or_else(too_large);
            price = ttc(price)?;
            if let Discount::Amount(off) = line.discount {
                line.discount = Discount::Amount(ttc(off)?);
            }
        }
        line.unit_price = Some(price);
        line.expected_unit_price = Some(price);
    }
    Ok(cart)
}

/// A stored draft as a `DraftDocument` (same line ids, so kit lines keep their parent), to be
/// rewritten with its settlement just before validation.
pub fn draft_of(doc: &Document) -> Result<DraftDocument, AppError> {
    let business_date = NaiveDate::parse_from_str(&doc.business_date, "%Y-%m-%d")
        .map_err(|_| AppError::Internal(format!("bad business date on document {}", doc.id)))?;
    let lines = doc
        .lines
        .iter()
        .map(|l| {
            let base = match doc.price_mode {
                PriceMode::Ttc => l.total_ttc,
                PriceMode::Ht => l.total_ht,
            };
            Ok(DraftLine {
                id: l.id.clone(),
                product_id: l.product_id.clone(),
                description: l.description.clone(),
                qty: l.qty,
                unit_code: l.unit_code.clone(),
                unit_price: l.unit_price,
                price_overridden: l.price_overridden,
                tax_rate_bp: l.tax_rate_bp,
                amounts: LineAmounts {
                    gross: base.checked_add(l.discount_amount).ok_or_else(too_large)?,
                    discount_bp: l.discount_bp,
                    discount_amount: l.discount_amount,
                    total_ht: l.total_ht,
                    tax: l.tax_amount,
                    total_ttc: l.total_ttc,
                },
                cost_price: l.cost_price,
                parent_line_id: l.parent_line_id.clone(),
                origin_line_id: l.origin_line_id.clone(),
                restock_location_id: l.restock_location_id.clone(),
                reason_code: l.reason_code.clone(),
                note: l.note.clone(),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(DraftDocument {
        doc_type: doc.doc_type,
        terminal_code: doc.series.clone(),
        business_date,
        customer_id: doc.customer_id.clone(),
        supplier_id: doc.supplier_id.clone(),
        user_id: doc.user_id.clone(),
        terminal_id: doc.terminal_id.clone(),
        cash_session_id: doc.cash_session_id.clone(),
        price_level_id: doc.price_level_id.clone(),
        lang: doc.lang,
        tax_regime: doc.tax_regime,
        price_mode: doc.price_mode,
        stamp_mode: doc.stamp_mode,
        stamp_duty: doc.stamp_duty,
        stamp_charged: if doc.stamp_mode == StampMode::ChargeCustomer {
            doc.stamp_duty
        } else {
            Money::ZERO
        },
        rounding: doc.rounding,
        paid_total: doc.paid_total,
        change_given: doc.change_given,
        credit_amount: doc.credit_amount,
        note: doc.note.clone(),
        parent_id: doc.parent_id.clone(),
        source_ref: doc.source_ref.clone(),
        lines,
        payments: doc
            .payments
            .iter()
            .map(|p| DraftPayment {
                method: p.method,
                amount: p.amount,
                reference: p.reference.clone(),
                cheque_bank: p.cheque_bank.clone(),
                cheque_date: p.cheque_date.clone(),
            })
            .collect(),
    })
}

/// Cancels a draft of the editor (M1a `cancel`, journaled `document.cancelled`).
pub fn cancel_draft(tx: &Transaction<'_>, ctx: &TillContext, id: &str) -> Result<(), AppError> {
    let doc = documents::get(tx, id)?;
    if !EDITOR_KINDS.contains(&doc.doc_type) {
        return Err(AppError::validation("id", "document_kind_invalid"));
    }
    documents::cancel(tx, &ctx.actor(), id)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::catalog::products::Product;
    use crate::company::TaxRegime;
    use crate::customers::{Customer, CustomerInput};
    use crate::money::Qty;
    use crate::sales::cart::CartLine;
    use crate::tax::tva::Discount;
    use crate::testkit::{Shop, shop};

    pub(crate) fn fixture(shop: &Shop) -> (Customer, Product, Product) {
        shop.complete_company();
        let amine = shop.b2b_customer("SARL Amine");
        let cement = shop.product("Ciment 25 kg", Money(119_000), &shop.tva19);
        let sand = shop.product("Sable 1 m3", Money(545_000), &shop.tva9);
        (amine, cement, sand)
    }

    pub(crate) fn cart(customer: Option<&Customer>, lines: Vec<CartLine>) -> Cart {
        let mut cart = Cart::new(Lang::Fr, lines);
        cart.customer_id = customer.map(|c| c.id.clone());
        cart
    }

    pub(crate) fn input(doc_type: DocType, cart: Cart) -> DraftInput {
        DraftInput {
            doc_type,
            cart,
            valid_until: None,
            due_date: None,
            second_lang: None,
            draft_id: None,
        }
    }

    pub(crate) fn save(
        shop: &Shop,
        id: Option<&str>,
        input: DraftInput,
    ) -> Result<Document, AppError> {
        let ctx = shop.owner_till();
        let id = id.map(str::to_owned);
        shop.db
            .write(move |tx| save_draft(tx, &ctx, id.as_deref(), input))
    }

    #[test]
    fn a_quote_draft_is_priced_like_the_till_and_keeps_its_cart() {
        let shop = shop();
        let (amine, cement, sand) = fixture(&shop);
        let mut ten = CartLine::product(&cement.id, Qty(10_000));
        ten.discount = Discount::Percent(Rate(500));
        let typed = cart(
            Some(&amine),
            vec![ten, CartLine::product(&sand.id, Qty(2_000))],
        );
        let mut quote = input(DocType::Quote, typed.clone());
        quote.valid_until = Some("2026-10-15".into());
        let doc = save(&shop, None, quote).unwrap();
        assert_eq!(
            (doc.doc_type, doc.status, doc.number.clone()),
            (DocType::Quote, DocStatus::Draft, None)
        );
        assert_eq!(
            (doc.total_ttc, doc.subtotal_ht, doc.tax_total),
            (Money(2_220_500), Money(1_950_000), Money(270_500)),
            "11 305.00 of cement (5 % off) and 10 900.00 of sand, TVA 19 % and 9 %"
        );
        assert_eq!(doc.valid_until.as_deref(), Some("2026-10-15"));
        assert_eq!(doc.origin, Some(DocOrigin::Direct));
        assert_eq!(doc.customer_id.as_deref(), Some(amine.id.as_str()));
        let id = doc.id.clone();
        let back = shop.db.read(move |c| draft_cart(c, &id)).unwrap();
        assert_eq!(back.cart, typed);
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'document.draft_saved'"),
            1
        );
        let ctx = shop.owner_till();
        let preview = shop
            .db
            .read(move |c| preview(c, &ctx, &input(DocType::Quote, typed)))
            .unwrap();
        assert_eq!(preview.priced.cart.totals.total_ttc, Money(2_220_500));
        assert!(preview.mentions.complete());
    }

    pub(crate) fn exempt_customer(shop: &Shop) -> Customer {
        let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
        shop.db
            .write(move |tx| {
                let mut input = CustomerInput::named("Entreprise nationale");
                input.address = Some("Zone industrielle, Rouiba".into());
                input.nif = Some("000016005555555".into());
                input.rc = Some("16/00-5555555B99".into());
                input.vat_exempt = true;
                input.vat_exempt_ref = Some("ATT-2026-114".into());
                crate::customers::create(tx, &actor, &pack, input)
            })
            .unwrap()
    }

    #[test]
    fn an_exempt_customer_pays_ht_without_tva() {
        let shop = shop();
        let (_, cement, sand) = fixture(&shop);
        let state = exempt_customer(&shop);
        let mut fixed = CartLine::product(&cement.id, Qty(1_000));
        fixed.discount = Discount::Amount(Money(11_900));
        let doc = save(
            &shop,
            None,
            input(
                DocType::Invoice,
                cart(
                    Some(&state),
                    vec![fixed, CartLine::product(&sand.id, Qty(1_000))],
                ),
            ),
        )
        .unwrap();
        assert!(
            doc.lines
                .iter()
                .all(|l| l.tax_rate_bp == Rate::ZERO && l.tax_amount == Money::ZERO)
        );
        assert_eq!(
            doc.lines[0].unit_price,
            Money(100_000),
            "1 190.00 TTC is 1 000.00 HT"
        );
        assert_eq!(
            doc.lines[0].discount_amount,
            Money(10_000),
            "119.00 TTC off is 100.00 HT off"
        );
        assert_eq!(
            doc.lines[1].total_ttc,
            Money(500_000),
            "5 450.00 TTC at 9 % is 5 000.00 HT"
        );
        assert_eq!(
            (doc.total_ttc, doc.tax_total),
            (Money(590_000), Money::ZERO)
        );

        let ht_shop = crate::testkit::shop_with(TaxRegime::Reel, PriceMode::Ht);
        ht_shop.complete_company();
        let state = exempt_customer(&ht_shop);
        let cement = ht_shop.product("Ciment 25 kg", Money(100_000), &ht_shop.tva19);
        let doc = save(
            &ht_shop,
            None,
            input(
                DocType::Quote,
                cart(
                    Some(&state),
                    vec![CartLine::product(&cement.id, Qty(1_000))],
                ),
            ),
        )
        .unwrap();
        assert_eq!(
            (doc.total_ttc, doc.tax_total),
            (Money(100_000), Money::ZERO)
        );
    }

    #[test]
    fn an_ifu_shop_prices_ttc_and_never_exempts() {
        let shop = crate::testkit::shop_with(TaxRegime::Ifu, PriceMode::Ttc);
        shop.complete_company();
        let state = exempt_customer(&shop);
        let cement = shop.product("Ciment 25 kg", Money(119_000), &shop.tva19);
        let ctx = shop.owner_till();
        let typed = cart(
            Some(&state),
            vec![CartLine::product(&cement.id, Qty(1_000))],
        );
        let priced = shop
            .db
            .read(move |c| pricing::price_document(c, &ctx, DocType::Invoice, &typed))
            .unwrap();
        assert!(!priced.vat_exempt);
        assert_eq!(priced.cart.totals.total_ttc, Money(119_000));
        assert_eq!(priced.cart.tax_regime, TaxRegime::Ifu);
    }

    #[test]
    fn editing_a_draft_keeps_its_id_and_origin_and_cancelling_ends_it() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        let first = save(
            &shop,
            None,
            input(
                DocType::DeliveryNote,
                cart(
                    Some(&amine),
                    vec![CartLine::product(&cement.id, Qty(2_000))],
                ),
            ),
        )
        .unwrap();
        let second = save(
            &shop,
            Some(&first.id),
            input(
                DocType::DeliveryNote,
                cart(
                    Some(&amine),
                    vec![CartLine::product(&cement.id, Qty(5_000))],
                ),
            ),
        )
        .unwrap();
        assert_eq!(second.id, first.id);
        assert_eq!(
            (second.total_ttc, second.origin),
            (Money(595_000), Some(DocOrigin::Direct))
        );
        let err = save(
            &shop,
            Some(&first.id),
            input(
                DocType::Quote,
                cart(
                    Some(&amine),
                    vec![CartLine::product(&cement.id, Qty(1_000))],
                ),
            ),
        )
        .unwrap_err();
        assert_eq!(err.code(), "document_kind_invalid");
        let (ctx, id) = (shop.owner_till(), first.id.clone());
        shop.db
            .write(move |tx| cancel_draft(tx, &ctx, &id))
            .unwrap();
        let err = save(
            &shop,
            Some(&first.id),
            input(
                DocType::DeliveryNote,
                cart(
                    Some(&amine),
                    vec![CartLine::product(&cement.id, Qty(1_000))],
                ),
            ),
        )
        .unwrap_err();
        assert_eq!(err.code(), "document_already_validated");
    }

    #[test]
    fn the_editor_gets_back_every_kind_of_line() {
        let shop = shop();
        let (amine, cement, sand) = fixture(&shop);
        let mut percent = CartLine::product(&cement.id, Qty(3_000));
        percent.discount = Discount::Percent(Rate(1_000));
        let mut fixed = CartLine::product(&sand.id, Qty(1_000));
        fixed.discount = Discount::Amount(Money(5_000));
        fixed.unit_price = Some(Money(540_000));
        let transport = CartLine::open_item(
            "Transport Alger – Blida",
            Qty(1_000),
            Money(300_000),
            &shop.tva19,
        );
        let typed = cart(Some(&amine), vec![percent, fixed, transport]);
        let doc = save(&shop, None, input(DocType::Invoice, typed.clone())).unwrap();
        let id = doc.id.clone();
        assert_eq!(
            shop.db.read(move |c| draft_cart(c, &id)).unwrap().cart,
            typed
        );
        let rebuilt = shop.db.read(move |c| cart_from_lines(c, &doc)).unwrap();
        assert_eq!(
            rebuilt.lines.len(),
            3,
            "a converted draft without a stored cart is rebuilt from its lines"
        );
        assert_eq!(
            rebuilt.lines[2].tax_rate_id.as_deref(),
            Some(shop.tva19.as_str())
        );
        assert_eq!(rebuilt.lines[1].discount, Discount::Amount(Money(5_000)));
    }

    #[test]
    fn a_stored_draft_is_written_back_unchanged_with_its_kit_lines() {
        use crate::catalog::UNIT_PIECE_ID;
        use crate::catalog::products::{Component, ProductInput};
        let shop = shop();
        let (amine, cement, sand) = fixture(&shop);
        let mut kit =
            ProductInput::simple("Kit dalle", UNIT_PIECE_ID, &shop.tva19, Money(1_000_000));
        kit.kind = ProductKind::Composite;
        kit.components = vec![
            Component {
                component_id: cement.id.clone(),
                qty: Qty(4_000),
            },
            Component {
                component_id: sand.id.clone(),
                qty: Qty(1_000),
            },
        ];
        let kit = shop.product_with(kit);
        let mut ten = CartLine::product(&cement.id, Qty(3_000));
        ten.discount = Discount::Percent(Rate(1_000));
        let typed = cart(
            Some(&amine),
            vec![CartLine::product(&kit.id, Qty(2_000)), ten],
        );
        let doc = save(&shop, None, input(DocType::Invoice, typed)).unwrap();
        let parents: Vec<_> = doc
            .lines
            .iter()
            .map(|l| l.parent_line_id.is_some())
            .collect();
        assert_eq!(
            parents,
            [false, true, true, false],
            "the kit, its two components, the cement"
        );
        assert_eq!(doc.lines[1].qty, Qty(8_000));
        let (ctx, stored) = (shop.owner_till(), doc.clone());
        let rewritten = shop
            .db
            .write(move |tx| {
                documents::replace_draft(tx, &stored.id, &draft_of(&stored)?, ctx.now)?;
                documents::get(tx, &stored.id)
            })
            .unwrap();
        assert_eq!(rewritten.lines, doc.lines, "same ids, parents and amounts");
        assert_eq!(rewritten.taxes, doc.taxes);
        assert_eq!(
            (rewritten.total_ttc, rewritten.total_due),
            (doc.total_ttc, doc.total_due)
        );

        let ctx = shop.owner_till();
        assert_eq!(add_days(&ctx, 30).unwrap(), "2026-10-28");
        assert_eq!(add_days(&ctx, 0).unwrap(), "2026-09-28");
        assert_eq!(add_days(&ctx, -1).unwrap_err().code(), "value_invalid");
    }

    #[test]
    fn drafts_need_the_right_kind_customer_dates_and_languages() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        let one =
            |who: Option<&Customer>| cart(who, vec![CartLine::product(&cement.id, Qty(1_000))]);
        let code = |i: DraftInput| {
            let e = save(&shop, None, i).unwrap_err();
            (e.payload().field.unwrap_or_default(), e.code())
        };
        assert_eq!(
            code(input(DocType::Ticket, one(Some(&amine)))),
            ("docType".into(), "document_kind_invalid".into())
        );
        assert_eq!(
            code(input(DocType::Invoice, one(None))),
            ("customerId".into(), "customer_required".into())
        );
        assert_eq!(
            code(input(DocType::DeliveryNote, one(None))),
            ("customerId".into(), "customer_required".into())
        );
        let mut i = input(DocType::Invoice, one(Some(&amine)));
        i.valid_until = Some("2026-10-15".into());
        assert_eq!(code(i), ("validUntil".into(), "value_invalid".into()));
        let mut i = input(DocType::Quote, one(None));
        i.valid_until = Some("2026-09-27".into());
        assert_eq!(code(i), ("validUntil".into(), "date_invalid".into()));
        let mut i = input(DocType::Quote, one(None));
        i.valid_until = Some("2026-10-5".into());
        assert_eq!(code(i), ("validUntil".into(), "date_invalid".into()));
        let mut i = input(DocType::Quote, one(None));
        i.due_date = Some("2026-10-15".into());
        assert_eq!(code(i), ("dueDate".into(), "value_invalid".into()));
        let mut i = input(DocType::Quote, one(None));
        i.second_lang = Some(Lang::Fr);
        assert_eq!(code(i), ("secondLang".into(), "value_invalid".into()));
        save(&shop, None, input(DocType::Quote, one(None))).expect("a quote may have no customer");
    }
}
