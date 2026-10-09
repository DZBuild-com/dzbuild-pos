//! One document made from another (ORD-001, DOC-031, SAL-054): a quote or proforma into an
//! invoice, a delivery note or a proforma (a draft, at the offered prices unless re-priced);
//! delivery notes of one customer into one invoice that moves no stock; a till ticket into its
//! invoice, which carries no turnover, stock, cash or account entry of its own; a quote sold at
//! the till. Every conversion leaves a link the archive and the states read.

use super::drafts::{cart_from_lines, check_day, draft_document, draft_of, offered_cart};
use super::invoices::{Finish, finish};
use super::mentions;
use super::pricing;
use super::settings::print_settings;
use super::validate::{ValidatedDocument, freeze};
use crate::audit;
use crate::catalog::clean_opt;
use crate::company::TaxRegime;
use crate::customers;
use crate::documents::links::{self, LinkKind};
use crate::documents::{self, DocOrigin, DocStatus, DocType, Document, DraftExtras, DraftLine};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::payments::{self, PaymentInput, Settlement};
use crate::sales::{Cart, cart};
use crate::tax::tva;
use crate::till::TillContext;
use rusqlite::{Connection, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

/// The most delivery notes one invoice gathers.
pub const MAX_DELIVERIES: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConvertInput {
    pub source_id: String,
    pub target: DocType,
    /// Price again at today's prices (required once the offer expired).
    #[serde(default)]
    pub reprice: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeliveriesInput {
    pub delivery_ids: Vec<String>,
    #[serde(default)]
    pub payments: Vec<PaymentInput>,
    #[serde(default)]
    #[specta(optional)]
    pub due_date: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub second_lang: Option<Lang>,
    #[serde(default)]
    #[specta(optional)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TicketInvoiceInput {
    pub ticket_id: String,
    pub customer_id: String,
    #[serde(default)]
    #[specta(optional)]
    pub second_lang: Option<Lang>,
    #[serde(default)]
    #[specta(optional)]
    pub note: Option<String>,
}

fn not_convertible() -> AppError {
    AppError::conflict("document_not_convertible")
}

/// A bilingual document prints two different languages (the editor's rule).
fn check_second_lang(lang: Lang, second: Option<Lang>) -> Result<(), AppError> {
    if second == Some(lang) {
        return Err(AppError::validation("secondLang", "value_invalid"));
    }
    Ok(())
}

/// Copies the lines of `docs`, in order, with new ids (a kit's component lines keep pointing
/// at their kit).
fn copy_lines(docs: &[&Document]) -> Result<Vec<DraftLine>, AppError> {
    let mut ids: BTreeMap<String, String> = BTreeMap::new();
    let mut out = Vec::new();
    for doc in docs {
        for line in draft_of(doc)?.lines {
            let id = new_id();
            ids.insert(line.id.clone(), id.clone());
            out.push(DraftLine {
                id,
                parent_line_id: line
                    .parent_line_id
                    .as_ref()
                    .and_then(|p| ids.get(p).cloned()),
                origin_line_id: None,
                restock_location_id: None,
                reason_code: None,
                ..line
            });
        }
    }
    Ok(out)
}

/// A validated quote or proforma (`document_not_convertible` otherwise) and whether its offer
/// ended before `today`.
fn offer(conn: &Connection, id: &str, today: &str) -> Result<(Document, bool), AppError> {
    let source = documents::get(conn, id)?;
    if source.status != DocStatus::Validated
        || !matches!(source.doc_type, DocType::Quote | DocType::Proforma)
    {
        return Err(not_convertible());
    }
    let expired = source.valid_until.as_deref().is_some_and(|v| v < today);
    Ok((source, expired))
}

/// A quote into an invoice, a delivery note or a proforma; a proforma into an invoice or a
/// delivery note; either into a customer order (ORD-001, validated by `orders::create`). The
/// result is a draft the editor opens: at the offered prices, or
/// priced again at today's (`reprice`, the only way once the offer expired).
pub fn convert(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: ConvertInput,
) -> Result<Document, AppError> {
    let (source, expired) = offer(tx, &input.source_id, &ctx.business_day())?;
    let allowed = matches!(
        (source.doc_type, input.target),
        (
            DocType::Quote,
            DocType::Invoice | DocType::DeliveryNote | DocType::Proforma
        ) | (DocType::Proforma, DocType::Invoice | DocType::DeliveryNote)
            | (DocType::Quote | DocType::Proforma, DocType::CustomerOrder)
    );
    if !allowed {
        return Err(not_convertible());
    }
    if expired && !input.reprice {
        return Err(AppError::conflict("quote_expired"));
    }
    if matches!(
        input.target,
        DocType::Invoice | DocType::DeliveryNote | DocType::CustomerOrder
    ) && source.customer_id.is_none()
    {
        return Err(AppError::validation("customerId", "customer_required"));
    }
    let origin = if source.doc_type == DocType::Quote {
        DocOrigin::Quote
    } else {
        DocOrigin::Proforma
    };
    // Re-priced, the draft keeps what was typed on the offer; at the offered prices, the editor
    // reopens every line at its offered price (saving it keeps those terms, `price_offer`).
    let cart: Cart = if input.reprice {
        match documents::stored_cart(tx, &source.id)? {
            Some(value) => serde_json::from_value(value)?,
            None => cart_from_lines(tx, &source)?,
        }
    } else {
        offered_cart(tx, &source)?
    };
    let draft = if input.reprice {
        let priced = pricing::price_document(tx, ctx, input.target, &cart)?;
        draft_document(tx, ctx, input.target, &cart, &priced)?
    } else {
        let mut draft = draft_of(&source)?;
        draft.doc_type = input.target;
        draft.terminal_code = ctx.session.terminal_code.clone();
        draft.business_date = ctx.business_date();
        draft.user_id = ctx.session.user_id.clone();
        draft.terminal_id = ctx.session.terminal_id.clone();
        draft.cash_session_id = None;
        draft.lines = copy_lines(&[&source])?;
        draft.payments = Vec::new();
        draft
    };
    let draft_id = documents::insert_draft(tx, &draft, ctx.now)?;
    documents::store_cart(tx, &draft_id, &serde_json::to_value(&cart)?, ctx.now)?;
    documents::set_extras(
        tx,
        &draft_id,
        &DraftExtras {
            second_lang: source.second_lang,
            origin: Some(origin),
            ..DraftExtras::default()
        },
    )?;
    links::add(tx, &source.id, &draft_id, LinkKind::Converted, ctx.now)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "document.converted",
            "document",
            Some(&draft_id),
            json!({
                "from": source.id,
                "fromNumber": source.number,
                "to": input.target,
                "reprice": input.reprice,
            }),
        ),
    )?;
    documents::get(tx, &draft_id)
}

/// The delivery notes `ids` as one invoice gathers them: 1 to [`MAX_DELIVERIES`], each validated
/// and not invoiced yet, all of one customer, regime and price mode.
fn deliveries_of(conn: &Connection, ids: &[String]) -> Result<Vec<Document>, AppError> {
    let mut seen = BTreeSet::new();
    if ids.is_empty()
        || ids.len() > MAX_DELIVERIES
        || !ids.iter().all(|id| seen.insert(id.as_str()))
    {
        return Err(AppError::validation("deliveryIds", "value_invalid"));
    }
    let mut deliveries = Vec::with_capacity(ids.len());
    for id in ids {
        let bl = documents::get(conn, id)?;
        if bl.status != DocStatus::Validated || bl.doc_type != DocType::DeliveryNote {
            return Err(not_convertible());
        }
        if links::invoiced(conn, &bl.id)?.is_some() {
            return Err(AppError::conflict("delivery_already_invoiced"));
        }
        deliveries.push(bl);
    }
    let first = &deliveries[0];
    if deliveries.iter().any(|d| {
        d.customer_id != first.customer_id
            || d.tax_regime != first.tax_regime
            || d.price_mode != first.price_mode
    }) {
        return Err(AppError::conflict("delivery_customer_mismatch"));
    }
    Ok(deliveries)
}

/// DOC-031: what the invoice of the delivery notes `delivery_ids` asks with `tenders`, as
/// [`invoice_deliveries`] would settle it: its TTC, the duty on the cash part, the net to pay,
/// what is left and the change (nothing written; the dialog never computes an amount).
pub fn deliveries_preview(
    conn: &Connection,
    ctx: &TillContext,
    delivery_ids: &[String],
    tenders: &[PaymentInput],
) -> Result<Settlement, AppError> {
    let deliveries = deliveries_of(conn, delivery_ids)?;
    let lines = copy_lines(&deliveries.iter().collect::<Vec<_>>())?;
    let totals = tva::totals(lines.iter().map(|l| (l.tax_rate_bp, &l.amounts)))?;
    payments::preview(
        &ctx.pack,
        totals.total_ttc,
        tenders,
        cart::settle_rules(conn)?,
    )
}

/// DOC-031: 1 to [`MAX_DELIVERIES`] validated delivery notes of one customer (one regime and
/// price mode) into one invoice, validated with its payments like any invoice. Their lines,
/// amounts and costs are copied in order; the stock does not move again (the notes took it).
/// Without a second language, the invoice takes the one set for new documents (DOC-034,
/// `documents.layout`) when it is not the notes' own.
pub fn invoice_deliveries(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: DeliveriesInput,
) -> Result<ValidatedDocument, AppError> {
    check_day("dueDate", &input.due_date, &ctx.business_day())?;
    let deliveries = deliveries_of(tx, &input.delivery_ids)?;
    let first = &deliveries[0];
    check_second_lang(first.lang, input.second_lang)?;
    let second_lang = match input.second_lang {
        Some(lang) => Some(lang),
        None => print_settings(tx)?
            .layout
            .second_lang
            .filter(|l| *l != first.lang),
    };
    let customer_id = first.customer_id.clone().ok_or_else(not_convertible)?;
    let mentions = mentions::require(tx, DocType::Invoice, Some(&customer_id))?;
    let customer = customers::get(tx, &customer_id)?;
    let mut draft = draft_of(first)?;
    draft.doc_type = DocType::Invoice;
    draft.terminal_code = ctx.session.terminal_code.clone();
    draft.business_date = ctx.business_date();
    draft.cash_session_id = None;
    draft.note = clean_opt(input.note.as_deref(), "note", 200)?;
    draft.lines = copy_lines(&deliveries.iter().collect::<Vec<_>>())?;
    draft.payments = Vec::new();
    let id = documents::insert_draft(tx, &draft, ctx.now)?;
    documents::set_extras(
        tx,
        &id,
        &DraftExtras {
            due_date: input.due_date.clone(),
            second_lang,
            origin: Some(DocOrigin::Delivery),
            ..DraftExtras::default()
        },
    )?;
    for bl in &deliveries {
        links::add(tx, &bl.id, &id, LinkKind::Delivered, ctx.now)?;
    }
    let doc = documents::get(tx, &id)?;
    finish(
        tx,
        ctx,
        doc,
        &customer,
        &input.payments,
        Finish {
            take_stock: false,
            refresh_costs: false,
            action: "invoice.validated",
            extra: json!({
                "deliveries": deliveries.iter().map(|d| d.number.clone()).collect::<Vec<_>>(),
            }),
        },
        mentions,
    )
}

/// SAL-054: the invoice of a till ticket, for a customer with every blocking mention: the one
/// who owes its kridi part (`ticket_customer_mismatch`), never a VAT-exempt buyer when the
/// ticket carries TVA (`ticket_vat_exempt_buyer`). Lines, totals, duty and the payment summary
/// are the ticket's; the ticket keeps the payments, the
/// cash, the stock, the account entry and the turnover, so the invoice books none of them.
pub fn invoice_ticket(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: TicketInvoiceInput,
) -> Result<ValidatedDocument, AppError> {
    let ticket = documents::get(tx, &input.ticket_id)?;
    if ticket.status != DocStatus::Validated || ticket.doc_type != DocType::Ticket {
        return Err(not_convertible());
    }
    if links::invoiced(tx, &ticket.id)?.is_some() {
        return Err(AppError::conflict("ticket_already_invoiced"));
    }
    let returned: i64 = tx.query_row(
        "SELECT COUNT(*) FROM documents
         WHERE parent_id = ?1 AND doc_type = 'return_note' AND status = 'validated'",
        [&ticket.id],
        |r| r.get(0),
    )?;
    if returned > 0 {
        return Err(AppError::conflict("ticket_returned"));
    }
    check_second_lang(ticket.lang, input.second_lang)?;
    let mentions = mentions::require(tx, DocType::Invoice, Some(&input.customer_id))?;
    let customer = customers::get(tx, &input.customer_id)?;
    // The kridi part of the ticket is owed by the ticket's customer: its invoice is his.
    if ticket.credit_amount.is_positive()
        && ticket
            .customer_id
            .as_deref()
            .is_some_and(|owner| owner != customer.id)
    {
        return Err(AppError::conflict("ticket_customer_mismatch"));
    }
    // A réel ticket always carries its TVA; an exempt buyer is invoiced HT without TVA
    // (TAX-004, décret 05-468 art. 3): the ticket is returned and a typed invoice made.
    if customer.vat_exempt && ticket.tax_regime == TaxRegime::Reel && ticket.tax_total.is_positive()
    {
        return Err(AppError::conflict("ticket_vat_exempt_buyer"));
    }
    // `draft_of` keeps the ticket's duty, its charged part, rounding and payment summary.
    let mut draft = draft_of(&ticket)?;
    draft.doc_type = DocType::Invoice;
    draft.terminal_code = ctx.session.terminal_code.clone();
    draft.business_date = ctx.business_date();
    draft.customer_id = Some(customer.id.clone());
    draft.user_id = ctx.session.user_id.clone();
    draft.terminal_id = ctx.session.terminal_id.clone();
    draft.cash_session_id = None;
    draft.note = clean_opt(input.note.as_deref(), "note", 200)?;
    draft.lines = copy_lines(&[&ticket])?;
    draft.payments = Vec::new();
    draft.parent_id = None;
    let id = documents::insert_draft(tx, &draft, ctx.now)?;
    documents::set_extras(
        tx,
        &id,
        &DraftExtras {
            second_lang: input.second_lang,
            origin: Some(DocOrigin::Ticket),
            ..DraftExtras::default()
        },
    )?;
    links::add(tx, &ticket.id, &id, LinkKind::InvoicedTicket, ctx.now)?;
    let doc = documents::get(tx, &id)?;
    if doc.total_due != ticket.total_due {
        return Err(AppError::Internal(format!(
            "invoice of ticket {} does not add up",
            ticket.id
        )));
    }
    freeze(tx, ctx, &doc, Some(&customer), false)?;
    let document = documents::validate(tx, &id, ctx.now)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "invoice.from_ticket",
            "document",
            Some(&document.id),
            json!({
                "number": document.number,
                "ticket": ticket.number,
                "customerId": customer.id,
                "totalDue": document.total_due,
            }),
        ),
    )?;
    Ok(ValidatedDocument {
        document,
        settlement: None,
        shortages: Vec::new(),
        customer_balance: None,
        mentions,
    })
}

/// SAL-046: the cart of a validated, unexpired quote or proforma for the till. The till prices
/// it at today's prices like any sale; the lines whose price changed since the offer carry the
/// offered price in `expected_unit_price` and are flagged; keeping the offered price is a price
/// override (`sales.price_override` or a manager approval). After the sale, [`link_sale`].
pub fn offer_cart(conn: &Connection, id: &str, today: &str) -> Result<Cart, AppError> {
    let (source, expired) = offer(conn, id, today)?;
    if expired {
        return Err(AppError::conflict("quote_expired"));
    }
    cart_from_lines(conn, &source)
}

/// A quote or proforma sold at the till (its lines were loaded into the cart by
/// [`offer_cart`]): the ticket is recorded as made from it.
pub fn link_sale(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    quote_id: &str,
    ticket_id: &str,
) -> Result<(), AppError> {
    let (quote, expired) = offer(tx, quote_id, &ctx.business_day())?;
    if expired {
        return Err(AppError::conflict("quote_expired"));
    }
    let ticket = documents::get(tx, ticket_id)?;
    if ticket.status != DocStatus::Validated || ticket.doc_type != DocType::Ticket {
        return Err(not_convertible());
    }
    links::add(tx, &quote.id, &ticket.id, LinkKind::SoldFromQuote, ctx.now)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "document.converted",
            "document",
            Some(&ticket.id),
            json!({
                "from": quote.id,
                "fromNumber": quote.number,
                "to": DocType::Ticket,
                "reprice": false,
            }),
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::drafts::tests::{cart, exempt_customer, fixture, input, save};
    use crate::billing::state::{DocState, state};
    use crate::billing::validate::{ValidateInput, validate};
    use crate::catalog::products::Product;
    use crate::company::PriceMode;
    use crate::customers::Customer;
    use crate::money::{Money, Qty};
    use crate::payments::{PaymentInput, PaymentMethod};
    use crate::returns::{ReturnInput, ReturnLineInput, ReturnReason, create_return};
    use crate::sales::cart::{Cart, CartLine};
    use crate::sales::{SaleInput, create_sale};
    use crate::tax::tva::Discount;
    use crate::testkit::{NOON, Shop, shop};

    fn validated(
        shop: &Shop,
        kind: DocType,
        who: &Customer,
        product: &Product,
        qty: i64,
    ) -> Document {
        let draft = save(
            shop,
            None,
            input(
                kind,
                cart(Some(who), vec![CartLine::product(&product.id, Qty(qty))]),
            ),
        )
        .unwrap();
        let ctx = shop.owner_till();
        shop.db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: Vec::new(),
                    },
                )
            })
            .unwrap()
            .document
    }

    fn transfer(amount: i64) -> PaymentInput {
        PaymentInput::electronic(
            PaymentMethod::Transfer,
            Money(amount),
            Some("VIR-2026-0931"),
        )
    }

    fn state_of_now(shop: &Shop, id: &str) -> DocState {
        let id = id.to_owned();
        shop.db.read(move |c| state(c, &id, "2026-09-28")).unwrap()
    }

    #[test]
    fn a_quote_becomes_an_invoice_draft_at_the_quoted_prices() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        let quote = validated(&shop, DocType::Quote, &amine, &cement, 10_000);
        let (actor, level, id) = (
            shop.owner.actor(),
            crate::catalog::RETAIL_LEVEL_ID.to_owned(),
            cement.id.clone(),
        );
        shop.db
            .write(move |tx| {
                crate::catalog::products::set_price(tx, &actor, &id, &level, Money(130_000))
                    .map(drop)
            })
            .unwrap();
        let (ctx, source) = (shop.owner_till(), quote.id.clone());
        let draft = shop
            .db
            .write(move |tx| {
                convert(
                    tx,
                    &ctx,
                    ConvertInput {
                        source_id: source,
                        target: DocType::Invoice,
                        reprice: false,
                    },
                )
            })
            .unwrap();
        assert_eq!(
            (draft.status, draft.origin, draft.total_ttc),
            (DocStatus::Draft, Some(DocOrigin::Quote), Money(1_190_000))
        );
        let ctx = shop.owner_till();
        let invoice = shop
            .db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: vec![transfer(1_190_000)],
                    },
                )
            })
            .unwrap()
            .document;
        assert_eq!(invoice.number.as_deref(), Some("FA2026-000001"));
        assert_eq!(
            state_of_now(&shop, &quote.id),
            DocState::Converted {
                into: vec!["FA2026-000001".into()]
            }
        );
    }

    fn convert_at_offer(shop: &Shop, source: &Document) -> Document {
        let (ctx, source) = (shop.owner_till(), source.id.clone());
        shop.db
            .write(move |tx| {
                convert(
                    tx,
                    &ctx,
                    ConvertInput {
                        source_id: source,
                        target: DocType::Invoice,
                        reprice: false,
                    },
                )
            })
            .unwrap()
    }

    #[test]
    fn a_converted_draft_keeps_the_offered_prices_when_previewed_and_saved() {
        use crate::billing::drafts::{DraftInput, draft_cart, preview, save_draft};
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        let mut ten = CartLine::product(&cement.id, Qty(10_000));
        ten.discount = Discount::Percent(crate::money::Rate(500));
        let quote = save(
            &shop,
            None,
            input(DocType::Quote, cart(Some(&amine), vec![ten])),
        )
        .unwrap();
        let ctx = shop.owner_till();
        let quote = shop
            .db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: quote.id,
                        payments: Vec::new(),
                    },
                )
            })
            .unwrap()
            .document;
        assert_eq!(quote.total_ttc, Money(1_130_500), "11 900.00 less 5 %");
        let (actor, level, id) = (
            shop.owner.actor(),
            crate::catalog::RETAIL_LEVEL_ID.to_owned(),
            cement.id.clone(),
        );
        shop.db
            .write(move |tx| {
                crate::catalog::products::set_price(tx, &actor, &id, &level, Money(130_000))
                    .map(drop)
            })
            .unwrap();
        let draft = convert_at_offer(&shop, &quote);
        let id = draft.id.clone();
        let opened = shop.db.read(move |c| draft_cart(c, &id)).unwrap();
        assert_eq!(opened.cart.lines[0].unit_price, Some(Money(119_000)));
        // The editor previews and saves (a due date typed) as the cashier, who may neither
        // override a price nor give 5 %: the offer's terms were agreed when it was validated.
        let edited = DraftInput {
            doc_type: DocType::Invoice,
            cart: opened.cart.clone(),
            valid_until: None,
            due_date: Some("2026-10-20".into()),
            second_lang: None,
            draft_id: Some(draft.id.clone()),
        };
        let (ctx, typed) = (shop.till(), edited.clone());
        let shown = shop.db.read(move |c| preview(c, &ctx, &typed)).unwrap();
        assert_eq!(shown.priced.cart.totals.total_ttc, Money(1_130_500));
        let (ctx, typed, id) = (shop.till(), edited.clone(), draft.id.clone());
        let saved = shop
            .db
            .write(move |tx| save_draft(tx, &ctx, Some(&id), typed))
            .unwrap();
        assert_eq!(
            (saved.total_ttc, saved.due_date.as_deref()),
            (Money(1_130_500), Some("2026-10-20"))
        );
        let ctx = shop.owner_till();
        let invoice = shop
            .db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: saved.id,
                        payments: vec![transfer(1_130_500)],
                    },
                )
            })
            .unwrap()
            .document;
        assert_eq!(invoice.total_ttc, Money(1_130_500), "the quoted total");

        // A price typed over the offered one is an override like any other.
        let draft = convert_at_offer(&shop, &quote);
        let mut changed = edited;
        changed.draft_id = Some(draft.id.clone());
        changed.cart.lines[0].unit_price = Some(Money(100_000));
        let (ctx, id) = (shop.till(), draft.id.clone());
        let err = shop
            .db
            .write(move |tx| save_draft(tx, &ctx, Some(&id), changed))
            .unwrap_err();
        assert_eq!(err.code(), "price_override_not_allowed");
    }

    #[test]
    fn an_exempt_buyers_quote_converts_at_its_ht_prices() {
        use crate::billing::drafts::{draft_cart, save_draft};
        let shop = shop();
        let (_, cement, _) = fixture(&shop);
        let state = exempt_customer(&shop);
        let mut off = CartLine::product(&cement.id, Qty(3_000));
        off.discount = Discount::Amount(Money(11_900));
        let quote = validated_cart(&shop, DocType::Quote, cart(Some(&state), vec![off]));
        assert_eq!(
            (quote.total_ttc, quote.tax_total),
            (Money(290_000), Money::ZERO),
            "3 000.00 HT less 100.00 HT"
        );
        let draft = convert_at_offer(&shop, &quote);
        let id = draft.id.clone();
        let opened = shop.db.read(move |c| draft_cart(c, &id)).unwrap();
        let (ctx, id) = (shop.till(), draft.id.clone());
        let saved = shop
            .db
            .write(move |tx| save_draft(tx, &ctx, Some(&id), input(DocType::Invoice, opened.cart)))
            .unwrap();
        assert_eq!(
            (saved.total_ttc, saved.tax_total),
            (Money(290_000), Money::ZERO)
        );
    }

    fn validated_cart(shop: &Shop, kind: DocType, typed: Cart) -> Document {
        let draft = save(shop, None, input(kind, typed)).unwrap();
        let ctx = shop.owner_till();
        shop.db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: Vec::new(),
                    },
                )
            })
            .unwrap()
            .document
    }

    #[test]
    fn an_expired_quote_converts_only_repriced() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        let quote = validated(&shop, DocType::Quote, &amine, &cement, 1_000);
        let (actor, level, id) = (
            shop.owner.actor(),
            crate::catalog::RETAIL_LEVEL_ID.to_owned(),
            cement.id.clone(),
        );
        shop.db
            .write(move |tx| {
                crate::catalog::products::set_price(tx, &actor, &id, &level, Money(130_000))
                    .map(drop)
            })
            .unwrap();
        let later = || {
            TillContext::new(
                shop.owner.clone(),
                shop.pack.clone(),
                NOON + 40 * 86_400_000,
            )
        };
        let (ctx, source) = (later(), quote.id.clone());
        let err = shop
            .db
            .write(move |tx| {
                convert(
                    tx,
                    &ctx,
                    ConvertInput {
                        source_id: source,
                        target: DocType::Invoice,
                        reprice: false,
                    },
                )
            })
            .unwrap_err();
        assert_eq!(err.code(), "quote_expired");
        let (ctx, source) = (later(), quote.id.clone());
        let draft = shop
            .db
            .write(move |tx| {
                convert(
                    tx,
                    &ctx,
                    ConvertInput {
                        source_id: source,
                        target: DocType::Invoice,
                        reprice: true,
                    },
                )
            })
            .unwrap();
        assert_eq!(draft.total_ttc, Money(130_000), "today's price");
    }

    #[test]
    fn invoicing_deliveries_never_moves_stock_again_and_cites_each_bl() {
        let shop = shop();
        let (amine, cement, sand) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        shop.stock_in(&sand.id, 100_000);
        let first = validated(&shop, DocType::DeliveryNote, &amine, &cement, 3_000);
        let second = validated(&shop, DocType::DeliveryNote, &amine, &sand, 2_000);
        let movements = shop.count("SELECT COUNT(*) FROM stock_movements");
        let ctx = shop.owner_till();
        let ids = vec![first.id.clone(), second.id.clone()];
        let done = shop
            .db
            .write(move |tx| {
                invoice_deliveries(
                    tx,
                    &ctx,
                    DeliveriesInput {
                        delivery_ids: ids,
                        payments: vec![transfer(1_447_000)],
                        due_date: None,
                        second_lang: None,
                        note: None,
                    },
                )
            })
            .unwrap();
        let invoice = &done.document;
        assert_eq!(invoice.origin, Some(DocOrigin::Delivery));
        assert_eq!(invoice.total_ttc, Money(357_000 + 1_090_000));
        assert_eq!(invoice.lines.len(), 2);
        assert_eq!(
            invoice.lines[0].cost_price, first.lines[0].cost_price,
            "the BL's cost"
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM stock_movements"),
            movements,
            "the BL moved the stock"
        );
        assert_eq!(
            state_of_now(&shop, &first.id),
            DocState::Invoiced {
                invoice: "FA2026-000001".into()
            }
        );
        let (ctx, ids) = (shop.owner_till(), vec![first.id.clone()]);
        let err = shop
            .db
            .write(move |tx| {
                invoice_deliveries(
                    tx,
                    &ctx,
                    DeliveriesInput {
                        delivery_ids: ids,
                        payments: vec![transfer(357_000)],
                        due_date: None,
                        second_lang: None,
                        note: None,
                    },
                )
            })
            .unwrap_err();
        assert_eq!(err.code(), "delivery_already_invoiced");
        let other = shop.b2b_customer("EURL Bâtir");
        let third = validated(&shop, DocType::DeliveryNote, &other, &cement, 1_000);
        let fourth = validated(&shop, DocType::DeliveryNote, &amine, &cement, 1_000);
        let (ctx, ids) = (shop.owner_till(), vec![third.id.clone(), fourth.id.clone()]);
        let err = shop
            .db
            .write(move |tx| {
                invoice_deliveries(
                    tx,
                    &ctx,
                    DeliveriesInput {
                        delivery_ids: ids,
                        payments: vec![transfer(238_000)],
                        due_date: None,
                        second_lang: None,
                        note: None,
                    },
                )
            })
            .unwrap_err();
        assert_eq!(err.code(), "delivery_customer_mismatch");
        assert!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'invoice.validated' AND payload LIKE '%BL2026-000001%'") == 1
        );
    }

    #[test]
    fn the_invoice_of_deliveries_is_previewed_as_it_is_settled_and_takes_the_second_language() {
        let shop = shop();
        let (amine, cement, sand) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        shop.stock_in(&sand.id, 100_000);
        let first = validated(&shop, DocType::DeliveryNote, &amine, &cement, 3_000);
        let second = validated(&shop, DocType::DeliveryNote, &amine, &sand, 2_000);
        let ids = vec![first.id.clone(), second.id.clone()];
        let (ctx, asked) = (shop.owner_till(), ids.clone());
        let cash = vec![PaymentInput::cash(Money(2_000_000))];
        let shown = shop
            .db
            .read(move |c| deliveries_preview(c, &ctx, &asked, &cash))
            .unwrap();
        assert_eq!(shown.total_ttc, Money(357_000 + 1_090_000));
        assert_eq!(
            shown.total_due,
            Money(357_000 + 1_090_000 + shown.stamp_charged.0 + shown.rounding.0),
            "the duty on the cash part, as the invoice will charge it"
        );
        assert!(shown.stamp_charged.is_positive());
        assert_eq!(shown.missing, Money::ZERO);
        let (ctx, asked) = (shop.owner_till(), vec![first.id.clone(), first.id.clone()]);
        assert_eq!(
            shop.db
                .read(move |c| deliveries_preview(c, &ctx, &asked, &[]))
                .unwrap_err()
                .code(),
            "value_invalid"
        );
        // Réglages › Documents: new documents are bilingual, Arabic second.
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| {
                let mut layout = crate::settings::get_value(tx, "documents.layout")?;
                layout["secondLang"] = json!("ar");
                crate::settings::set_value(tx, &actor, "documents.layout", layout).map(drop)
            })
            .unwrap();
        // the cash goes into this terminal's drawer
        shop.open_till(0);
        let (ctx, tenders) = (
            shop.owner_till(),
            vec![PaymentInput::cash(Money(2_000_000))],
        );
        let done = shop
            .db
            .write(move |tx| {
                invoice_deliveries(
                    tx,
                    &ctx,
                    DeliveriesInput {
                        delivery_ids: ids,
                        payments: tenders,
                        due_date: None,
                        second_lang: None,
                        note: None,
                    },
                )
            })
            .unwrap();
        assert_eq!(
            done.document.total_due, shown.total_due,
            "what the preview said"
        );
        assert_eq!(done.document.second_lang, Some(Lang::Ar));
    }

    fn cash_ticket(shop: &Shop, product: &Product, qty: i64, cash: i64) -> Document {
        let ctx = shop.till();
        let input = SaleInput {
            cart: Cart::new(Lang::Fr, vec![CartLine::product(&product.id, Qty(qty))]),
            payments: vec![PaymentInput::cash(Money(cash))],
        };
        shop.db
            .write(move |tx| create_sale(tx, &ctx, input))
            .unwrap()
            .document
    }

    #[test]
    fn a_ticket_is_invoiced_once_and_never_counted_twice() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        shop.open_till(100_000);
        let ticket = cash_ticket(&shop, &cement, 2_000, 250_000);
        assert_eq!(
            (ticket.total_due, ticket.stamp_duty),
            (Money(240_400), Money(2_400))
        );
        let movements = shop.count("SELECT COUNT(*) FROM stock_movements");
        let cash = shop.count("SELECT COUNT(*) FROM cash_movements");
        let (ctx, ticket_id, customer_id) =
            (shop.owner_till(), ticket.id.clone(), amine.id.clone());
        let done = shop
            .db
            .write(move |tx| {
                invoice_ticket(
                    tx,
                    &ctx,
                    TicketInvoiceInput {
                        ticket_id,
                        customer_id,
                        second_lang: None,
                        note: None,
                    },
                )
            })
            .unwrap();
        let invoice = done.document;
        assert_eq!(invoice.origin, Some(DocOrigin::Ticket));
        assert_eq!(
            (
                invoice.total_ttc,
                invoice.stamp_duty,
                invoice.total_due,
                invoice.paid_total,
                invoice.change_given
            ),
            (
                ticket.total_ttc,
                ticket.stamp_duty,
                ticket.total_due,
                ticket.paid_total,
                ticket.change_given
            )
        );
        assert!(invoice.payments.is_empty(), "the ticket holds the payments");
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM stock_movements"),
            movements
        );
        assert_eq!(shop.count("SELECT COUNT(*) FROM cash_movements"), cash);
        assert_eq!(invoice.parties[1].nif.as_deref(), Some("000016001234567"));
        assert_eq!(
            state_of_now(&shop, &ticket.id),
            DocState::Invoiced {
                invoice: invoice.number.clone().unwrap()
            }
        );
        let (ctx, ticket_id, customer_id) =
            (shop.owner_till(), ticket.id.clone(), amine.id.clone());
        let err = shop
            .db
            .write(move |tx| {
                invoice_ticket(
                    tx,
                    &ctx,
                    TicketInvoiceInput {
                        ticket_id,
                        customer_id,
                        second_lang: None,
                        note: None,
                    },
                )
            })
            .unwrap_err();
        assert_eq!(err.code(), "ticket_already_invoiced");
        let ctx = shop.till();
        let back = ReturnInput {
            original_id: Some(ticket.id.clone()),
            lines: vec![ReturnLineInput {
                origin_line_id: Some(ticket.lines[0].id.clone()),
                product_id: None,
                qty: Qty(1_000),
                unit_price: None,
                restock: true,
                reason: ReturnReason::Defective,
                note: None,
            }],
            refund_method: PaymentMethod::Cash,
            refund_reference: None,
            lang: Lang::Fr,
            note: None,
        };
        let err = shop
            .db
            .write(move |tx| create_return(tx, &ctx, back))
            .unwrap_err();
        assert_eq!(err.code(), "ticket_invoiced");
    }

    #[test]
    fn a_returned_ticket_cannot_be_invoiced() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        shop.open_till(100_000);
        let ticket = cash_ticket(&shop, &cement, 1_000, 121_000);
        let ctx = shop.till();
        let back = ReturnInput {
            original_id: Some(ticket.id.clone()),
            lines: vec![ReturnLineInput {
                origin_line_id: Some(ticket.lines[0].id.clone()),
                product_id: None,
                qty: Qty(1_000),
                unit_price: None,
                restock: true,
                reason: ReturnReason::ChangedMind,
                note: None,
            }],
            refund_method: PaymentMethod::Cash,
            refund_reference: None,
            lang: Lang::Fr,
            note: None,
        };
        shop.db
            .write(move |tx| create_return(tx, &ctx, back))
            .unwrap();
        let (ctx, ticket_id, customer_id) =
            (shop.owner_till(), ticket.id.clone(), amine.id.clone());
        let err = shop
            .db
            .write(move |tx| {
                invoice_ticket(
                    tx,
                    &ctx,
                    TicketInvoiceInput {
                        ticket_id,
                        customer_id,
                        second_lang: None,
                        note: None,
                    },
                )
            })
            .unwrap_err();
        assert_eq!(err.code(), "ticket_returned");
    }

    #[test]
    fn an_invoiced_ticket_is_corrected_by_a_credit_note_only() {
        use crate::billing::credit_notes::{
            CreditLineInput, CreditNoteInput, CreditReason, create_credit_note,
        };
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        shop.open_till(100_000);
        let ticket = cash_ticket(&shop, &cement, 2_000, 250_000);
        let (ctx, ticket_id, customer_id) =
            (shop.owner_till(), ticket.id.clone(), amine.id.clone());
        let invoice = shop
            .db
            .write(move |tx| {
                invoice_ticket(
                    tx,
                    &ctx,
                    TicketInvoiceInput {
                        ticket_id,
                        customer_id,
                        second_lang: None,
                        note: None,
                    },
                )
            })
            .unwrap()
            .document;
        let stock = shop.count("SELECT SUM(qty_delta) FROM stock_movements");
        let ctx = shop.owner_till();
        let input = CreditNoteInput {
            invoice_id: invoice.id.clone(),
            kind: crate::documents::CreditKind::Goods,
            lines: vec![CreditLineInput {
                origin_line_id: invoice.lines[0].id.clone(),
                qty: Qty(1_000),
                restock: true,
            }],
            amount: None,
            reason: CreditReason::Defective,
            refund_method: PaymentMethod::Cash,
            refund_reference: None,
            note: None,
            second_lang: None,
        };
        let av = shop
            .db
            .write(move |tx| create_credit_note(tx, &ctx, input))
            .unwrap();
        assert_eq!(av.refund, Money(119_000));
        assert_eq!(
            shop.count("SELECT SUM(qty_delta) FROM stock_movements"),
            stock + 1_000
        );
    }

    /// Finding 39: the demo shop numbers no invoice and no credit note; its tickets stay
    /// tickets with the "DÉMO – sans valeur fiscale" mark.
    #[test]
    fn the_demo_shop_numbers_no_invoice_and_no_credit_note() {
        use crate::billing::credit_notes::{
            CreditLineInput, CreditNoteInput, CreditReason, create_credit_note,
        };
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        shop.open_till(100_000);
        let earlier = cash_ticket(&shop, &cement, 1_000, 250_000);
        // An invoice made before the database was marked (a credit note against it is refused
        // all the same).
        let invoice = invoice_to(&shop, &earlier, &amine).unwrap();
        shop.db.write(|tx| crate::demo::mark(tx)).unwrap();
        let ticket = cash_ticket(&shop, &cement, 1_000, 250_000);
        assert_eq!(
            invoice_to(&shop, &ticket, &amine).unwrap_err().code(),
            "demo_not_allowed"
        );
        let ctx = shop.owner_till();
        let input = CreditNoteInput {
            invoice_id: invoice.id.clone(),
            kind: crate::documents::CreditKind::Goods,
            lines: vec![CreditLineInput {
                origin_line_id: invoice.lines[0].id.clone(),
                qty: Qty(1_000),
                restock: true,
            }],
            amount: None,
            reason: CreditReason::Defective,
            refund_method: PaymentMethod::Cash,
            refund_reference: None,
            note: None,
            second_lang: None,
        };
        assert_eq!(
            shop.db
                .write(move |tx| create_credit_note(tx, &ctx, input))
                .unwrap_err()
                .code(),
            "demo_not_allowed"
        );
        assert_eq!(
            (
                shop.count("SELECT COUNT(*) FROM documents WHERE doc_type = 'invoice'"),
                shop.count("SELECT COUNT(*) FROM documents WHERE doc_type = 'credit_note'"),
            ),
            (1, 0),
            "nothing numbered, nothing left behind"
        );
    }

    #[test]
    fn a_quote_sold_at_the_till_is_linked_to_its_ticket() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        shop.open_till(100_000);
        let quote = validated(&shop, DocType::Quote, &amine, &cement, 1_000);
        let today = shop.owner_till().business_day();
        let (q0, day) = (quote.id.clone(), today.clone());
        let cart = shop.db.read(move |c| offer_cart(c, &q0, &day)).unwrap();
        assert_eq!(
            cart.customer_id.as_deref(),
            Some(amine.id.as_str()),
            "the till gets the quote's customer"
        );
        assert_eq!(cart.lines.len(), quote.lines.len());
        assert_eq!(cart.lines[0].qty, quote.lines[0].qty);
        assert_eq!(
            cart.lines[0].expected_unit_price,
            Some(quote.lines[0].unit_price),
            "a price changed since is flagged"
        );
        let ticket = cash_ticket(&shop, &cement, 1_000, 121_000);
        let (t0, day) = (ticket.id.clone(), today);
        assert_eq!(
            shop.db
                .read(move |c| offer_cart(c, &t0, &day))
                .unwrap_err()
                .code(),
            "document_not_convertible"
        );
        let (ctx, q, t) = (shop.owner_till(), quote.id.clone(), ticket.id.clone());
        shop.db
            .write(move |tx| link_sale(tx, &ctx, &q, &t))
            .unwrap();
        assert_eq!(
            state_of_now(&shop, &quote.id),
            DocState::Converted {
                into: vec![ticket.number.clone().unwrap()]
            }
        );
        let (ctx, q, t) = (shop.owner_till(), ticket.id.clone(), quote.id.clone());
        let err = shop
            .db
            .write(move |tx| link_sale(tx, &ctx, &q, &t))
            .unwrap_err();
        assert_eq!(err.code(), "document_not_convertible");
    }

    #[test]
    fn an_open_price_line_of_a_quote_reaches_the_till_at_its_typed_price() {
        // SAL-019 + SAL-046: a department key (like a repair service) is priced when it is sold,
        // so its typed price is its price, not an override; the quote sold at the till keeps it.
        use crate::catalog::departments::{self, DepartmentKeyInput};
        let shop = shop();
        let (amine, _, _) = fixture(&shop);
        let (actor, rate) = (shop.owner.actor(), shop.tva19.clone());
        let key = shop
            .db
            .write(move |tx| {
                departments::create(
                    tx,
                    &actor,
                    DepartmentKeyInput {
                        name_ar: "تصليح".into(),
                        name_fr: "Réparation".into(),
                        name_en: "Repair".into(),
                        category_id: None,
                        tax_rate_id: rate,
                        unit_id: None,
                        color: None,
                        sort_order: 1,
                    },
                )
            })
            .unwrap();
        let id = key.id.clone();
        let typed = shop
            .db
            .read(move |c| departments::line(c, &id, Money(300_000), Qty(1_000)))
            .unwrap();
        let draft = save(
            &shop,
            None,
            input(DocType::Quote, cart(Some(&amine), vec![typed])),
        )
        .unwrap();
        let ctx = shop.owner_till();
        let quote = shop
            .db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: Vec::new(),
                    },
                )
            })
            .unwrap()
            .document;
        assert_eq!(
            (quote.lines[0].unit_price, quote.lines[0].price_overridden),
            (Money(300_000), false)
        );
        let (q, day) = (quote.id.clone(), shop.owner_till().business_day());
        let offered = shop.db.read(move |c| offer_cart(c, &q, &day)).unwrap();
        assert_eq!(
            offered.lines[0].unit_price,
            Some(Money(300_000)),
            "the typed price travels with the line"
        );
        let ctx = shop.till();
        let priced = shop
            .db
            .read(move |c| crate::sales::cart::price_cart(c, &ctx, &offered))
            .unwrap();
        let l = &priced.lines[0];
        assert_eq!(
            (l.unit_price, l.price_overridden, l.price_changed),
            (Money(300_000), false, false),
            "a cashier sells it at the quoted price without override rights"
        );
    }

    fn invoice_to(
        shop: &Shop,
        ticket: &Document,
        customer: &Customer,
    ) -> Result<Document, AppError> {
        let (ctx, ticket_id, customer_id) =
            (shop.owner_till(), ticket.id.clone(), customer.id.clone());
        shop.db
            .write(move |tx| {
                invoice_ticket(
                    tx,
                    &ctx,
                    TicketInvoiceInput {
                        ticket_id,
                        customer_id,
                        second_lang: None,
                        note: None,
                    },
                )
            })
            .map(|done| done.document)
    }

    fn no_invoice_numbered(shop: &Shop) {
        assert_eq!(
            (
                shop.count("SELECT COUNT(*) FROM counters WHERE doc_type = 'invoice'"),
                shop.count("SELECT COUNT(*) FROM documents WHERE doc_type = 'invoice'"),
            ),
            (0, 0),
            "no FA number consumed, no invoice left behind"
        );
    }

    #[test]
    fn a_ticket_with_tva_is_never_invoiced_to_a_vat_exempt_buyer() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        let exempt = exempt_customer(&shop);
        shop.stock_in(&cement.id, 100_000);
        shop.open_till(100_000);
        let ticket = cash_ticket(&shop, &cement, 1_000, 121_000);
        assert!(
            ticket.tax_total.is_positive(),
            "a till ticket keeps its TVA"
        );
        let err = invoice_to(&shop, &ticket, &exempt).unwrap_err();
        assert_eq!(err.code(), "ticket_vat_exempt_buyer");
        no_invoice_numbered(&shop);
        let invoice = invoice_to(&shop, &ticket, &amine).unwrap();
        assert_eq!(invoice.number.as_deref(), Some("FA2026-000001"));

        // An IFU shop never shows TVA and never exempts: its ticket invoices to anyone.
        let ifu = crate::testkit::shop_with(crate::company::TaxRegime::Ifu, PriceMode::Ttc);
        ifu.complete_company();
        let exempt = exempt_customer(&ifu);
        let cement = ifu.product("Ciment 25 kg", Money(119_000), &ifu.tva19);
        ifu.stock_in(&cement.id, 100_000);
        ifu.open_till(100_000);
        let ticket = cash_ticket(&ifu, &cement, 1_000, 121_000);
        let invoice = invoice_to(&ifu, &ticket, &exempt).unwrap();
        assert_eq!(invoice.total_due, ticket.total_due);
    }

    #[test]
    fn a_kridi_ticket_is_invoiced_only_to_the_customer_who_owes_it() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        let other = shop.b2b_customer("EURL Bâtir");
        shop.stock_in(&cement.id, 100_000);
        shop.open_till(100_000);
        let ctx = shop.till();
        let mut on_credit = Cart::new(Lang::Fr, vec![CartLine::product(&cement.id, Qty(1_000))]);
        on_credit.customer_id = Some(amine.id.clone());
        let sale = SaleInput {
            cart: on_credit,
            payments: vec![PaymentInput::electronic(
                PaymentMethod::Credit,
                Money(119_000),
                None,
            )],
        };
        let ticket = shop
            .db
            .write(move |tx| create_sale(tx, &ctx, sale))
            .unwrap()
            .document;
        assert_eq!(ticket.credit_amount, Money(119_000));
        let err = invoice_to(&shop, &ticket, &other).unwrap_err();
        assert_eq!(err.code(), "ticket_customer_mismatch");
        no_invoice_numbered(&shop);
        let invoice = invoice_to(&shop, &ticket, &amine).unwrap();
        assert_eq!(invoice.customer_id.as_deref(), Some(amine.id.as_str()));
        assert_eq!(
            state_of_now(&shop, &invoice.id),
            DocState::Unpaid {
                remaining: Money(119_000),
                due_date: None,
                overdue: false
            },
            "the invoice reads the debt of its own customer"
        );
    }

    fn field_code(err: AppError) -> (String, String) {
        (err.payload().field.unwrap_or_default(), err.code())
    }

    #[test]
    fn conversions_check_their_source_target_and_inputs() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        let proforma = validated(&shop, DocType::Proforma, &amine, &cement, 1_000);
        let bl = validated(&shop, DocType::DeliveryNote, &amine, &cement, 1_000);
        let draft = save(
            &shop,
            None,
            input(
                DocType::Quote,
                cart(None, vec![CartLine::product(&cement.id, Qty(1_000))]),
            ),
        )
        .unwrap();
        let ctx = shop.owner_till();
        let anonymous = shop
            .db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: Vec::new(),
                    },
                )
            })
            .unwrap()
            .document;
        let convert_to = |source: &Document, target: DocType, reprice: bool| {
            let (ctx, source_id) = (shop.owner_till(), source.id.clone());
            shop.db.write(move |tx| {
                convert(
                    tx,
                    &ctx,
                    ConvertInput {
                        source_id,
                        target,
                        reprice,
                    },
                )
            })
        };
        let refused = |source: &Document, target: DocType| {
            field_code(convert_to(source, target, false).unwrap_err())
        };
        let not_convertible = (String::new(), "document_not_convertible".to_owned());
        assert_eq!(refused(&proforma, DocType::Quote), not_convertible);
        assert_eq!(refused(&bl, DocType::Invoice), not_convertible);
        assert_eq!(
            refused(&anonymous, DocType::Invoice),
            ("customerId".into(), "customer_required".into())
        );
        let audits = shop.count("SELECT COUNT(*) FROM audit_log");
        let draft = convert_to(&proforma, DocType::DeliveryNote, true).unwrap();
        assert_eq!(
            (draft.doc_type, draft.origin),
            (DocType::DeliveryNote, Some(DocOrigin::Proforma))
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log") - audits,
            1,
            "a conversion is journaled once, re-priced or not"
        );

        let deliver = |ids: Vec<String>, due_date: Option<&str>, second_lang: Option<Lang>| {
            let (ctx, due_date) = (shop.owner_till(), due_date.map(str::to_owned));
            let err = shop
                .db
                .write(move |tx| {
                    invoice_deliveries(
                        tx,
                        &ctx,
                        DeliveriesInput {
                            delivery_ids: ids,
                            payments: vec![transfer(119_000)],
                            due_date,
                            second_lang,
                            note: None,
                        },
                    )
                })
                .unwrap_err();
            field_code(err)
        };
        let ids_invalid = ("deliveryIds".to_owned(), "value_invalid".to_owned());
        assert_eq!(deliver(Vec::new(), None, None), ids_invalid);
        assert_eq!(
            deliver(vec![bl.id.clone(), bl.id.clone()], None, None),
            ids_invalid
        );
        assert_eq!(
            deliver(vec![bl.id.clone()], Some("2026-09-27"), None),
            ("dueDate".into(), "date_invalid".into())
        );
        assert_eq!(
            deliver(vec![bl.id.clone()], None, Some(Lang::Fr)),
            ("secondLang".into(), "value_invalid".into())
        );
        assert_eq!(
            deliver(vec![proforma.id.clone()], None, None),
            not_convertible
        );
    }
}
