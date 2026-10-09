//! Invoices (DOC-002, SAL-054, TAX-006/007): validated with their payments — the stamp duty on
//! the cash part (M1a `settle`), the credit part on the customer's account (M2, dutiable at its
//! cash versement), the cash in this terminal's drawer, the stock out — and never while a
//! blocking mention of décret 05-468 is missing (DOC-003).

use super::drafts::{add_days, check_day, draft_of};
use super::mentions::{self, MentionCheck};
use super::settings::invoice_due_days;
use super::validate::{ValidatedDocument, freeze, take_stock};
use crate::audit;
use crate::cash;
use crate::catalog::{products, tax_rates};
use crate::company::{self, TaxRegime};
use crate::customers::{self, Customer};
use crate::documents::{self, DocOrigin, DocStatus, DocType, Document, DraftPayment};
use crate::error::AppError;
use crate::money::Rate;
use crate::payments::{self, PaymentInput, PaymentMethod, Settlement};
use crate::sales::cart::{self, Cart};
use crate::till::TillContext;
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};

fn amount_err() -> AppError {
    AppError::validation("payments", "amount_invalid")
}

/// The live settlement of the validation dialog (duty, change, what is left).
pub fn payment_preview(
    conn: &Connection,
    ctx: &TillContext,
    id: &str,
    tenders: &[PaymentInput],
) -> Result<Settlement, AppError> {
    let doc = documents::get(conn, id)?;
    if doc.status != DocStatus::Draft {
        return Err(AppError::conflict("document_already_validated"));
    }
    if doc.doc_type != DocType::Invoice {
        return Err(AppError::validation("id", "document_kind_invalid"));
    }
    payments::preview(&ctx.pack, doc.total_ttc, tenders, cart::settle_rules(conn)?)
}

/// How [`finish`] ends an invoice: whether its lines leave the stock (not when delivery notes
/// already took it) at today's CMUP, and the audit action with the caller's extra fields.
pub(crate) struct Finish {
    pub take_stock: bool,
    pub refresh_costs: bool,
    pub action: &'static str,
    pub extra: Value,
}

/// An invoice typed in the editor or made from a quote or proforma (a customer and every
/// blocking mention, then [`finish`]).
pub(crate) fn validate_invoice(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    doc: Document,
    payments: Vec<PaymentInput>,
) -> Result<ValidatedDocument, AppError> {
    let customer_id = doc
        .customer_id
        .clone()
        .ok_or_else(|| AppError::validation("customerId", "customer_required"))?;
    let mentions = mentions::require(tx, DocType::Invoice, Some(&customer_id))?;
    let customer = customers::get(tx, &customer_id)?;
    let moves_stock = !matches!(doc.origin, Some(DocOrigin::Delivery | DocOrigin::Ticket));
    finish(
        tx,
        ctx,
        doc,
        &customer,
        &payments,
        Finish {
            take_stock: moves_stock,
            refresh_costs: moves_stock,
            action: "invoice.validated",
            extra: json!({}),
        },
        mentions,
    )
}

/// The payment rows of a settlement: the tenders, then the change given back (negative).
fn draft_payments(settlement: &Settlement) -> Result<Vec<DraftPayment>, AppError> {
    let mut rows: Vec<DraftPayment> = settlement
        .tenders
        .iter()
        .map(|t| DraftPayment {
            method: t.method,
            amount: t.amount,
            reference: t.reference.clone(),
            cheque_bank: t.cheque_bank.clone(),
            cheque_date: t.cheque_date.clone(),
        })
        .collect();
    if settlement.change.is_positive() {
        rows.push(DraftPayment {
            method: PaymentMethod::Change,
            amount: settlement.change.checked_neg().ok_or_else(amount_err)?,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
        });
    }
    Ok(rows)
}

/// The invoice is numbered in the tax context its lines were priced in (TAX-004, décret 05-468
/// art. 3; CTCA art. 64): the shop's regime has not changed since, an exempt buyer's lines are
/// all at 0 %, and no line of a buyer who is no longer exempt (or never was) was zero-rated by an
/// exemption — a product line at 0 % whose product carries a rate, an open item at 0 % typed at a
/// rate. Otherwise `document_tax_context_changed`: the draft is saved again (priced anew) first.
pub(crate) fn check_tax_context(
    conn: &Connection,
    doc: &Document,
    customer: &Customer,
) -> Result<(), AppError> {
    let changed = || AppError::conflict("document_tax_context_changed");
    let shop = company::get(conn)?.ok_or_else(|| AppError::conflict("setup_required"))?;
    if doc.tax_regime != shop.tax_regime {
        return Err(changed());
    }
    if shop.tax_regime == TaxRegime::Reel && customer.vat_exempt {
        if doc.lines.iter().any(|l| l.tax_rate_bp != Rate::ZERO) {
            return Err(changed());
        }
        return Ok(());
    }
    let typed: Option<Cart> = documents::stored_cart(conn, &doc.id)?
        .map(serde_json::from_value)
        .transpose()?;
    let tops = doc.lines.iter().filter(|l| l.parent_line_id.is_none());
    for (i, line) in tops.enumerate() {
        if line.tax_rate_bp != Rate::ZERO {
            continue;
        }
        let rate_id = match &line.product_id {
            Some(id) => Some(products::get(conn, id)?.tax_rate_id),
            None => typed
                .as_ref()
                .and_then(|c| c.lines.get(i))
                .filter(|l| l.product_id.is_none())
                .and_then(|l| l.tax_rate_id.clone()),
        };
        if let Some(id) = rate_id
            && tax_rates::get(conn, &id)?.rate_bp != Rate::ZERO
        {
            return Err(changed());
        }
    }
    Ok(())
}

/// Settles, freezes, numbers and posts an invoice draft (typed, from quotes or from delivery
/// notes) in the caller's transaction. A due date that has passed since the draft was saved is
/// refused (`dueDate`/`date_invalid`): the invoice is re-dated to today and its settlement date
/// is never earlier.
pub(crate) fn finish(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    doc: Document,
    customer: &Customer,
    tenders: &[PaymentInput],
    how: Finish,
    mentions: MentionCheck,
) -> Result<ValidatedDocument, AppError> {
    check_day("dueDate", &doc.due_date, &ctx.business_day())?;
    check_tax_context(tx, &doc, customer)?;
    let settlement = payments::settle(&ctx.pack, doc.total_ttc, tenders, cart::settle_rules(tx)?)?;
    let account = customers::credit::check_account_tenders(tx, ctx, Some(&customer.id), tenders)?;
    // This till's drawer, or the calling till's for a host service (Task 20 review).
    let session = if settlement.cash_tendered.is_positive() {
        Some(cash::drawer(tx, ctx)?)
    } else {
        cash::drawer_if_open(tx, ctx)?
    };
    let mut draft = draft_of(&doc)?;
    draft.user_id = ctx.session.user_id.clone();
    draft.terminal_id = ctx.session.terminal_id.clone();
    draft.cash_session_id = session.as_ref().map(|s| s.id.clone());
    draft.stamp_mode = settlement.stamp_mode;
    draft.stamp_duty = settlement.stamp_duty;
    draft.stamp_charged = settlement.stamp_charged;
    draft.rounding = settlement.rounding;
    draft.paid_total = settlement.paid_total;
    draft.change_given = settlement.change;
    draft.credit_amount = account.credit;
    draft.payments = draft_payments(&settlement)?;
    documents::replace_draft(tx, &doc.id, &draft, ctx.now)?;
    let mut extras = documents::extras_of(&doc);
    if account.credit.is_positive() && extras.due_date.is_none() {
        extras.due_date = Some(add_days(ctx, invoice_due_days(tx)?)?);
    }
    documents::set_extras(tx, &doc.id, &extras)?;
    let doc = documents::get(tx, &doc.id)?;
    freeze(tx, ctx, &doc, Some(customer), how.refresh_costs)?;
    let document = documents::validate(tx, &doc.id, ctx.now)?;
    let shortages = if how.take_stock {
        take_stock(tx, ctx, &document)?
    } else {
        Vec::new()
    };
    if let (Some(share), Some(s)) = (
        cash::DrawerShare::of_settlement(&document.id, &settlement)?,
        &session,
    ) {
        cash::book(tx, ctx, &s.id, &share)?;
    }
    let customer_balance = customers::credit::book_sale(tx, ctx, &account, &document.id)?;
    let mut payload = json!({
        "number": document.number,
        "totalDue": document.total_due,
        "stampDuty": document.stamp_duty,
        "credit": account.credit,
        "customerId": customer.id,
        "origin": document.origin,
        "approvals": ctx.approvers(),
        "shortages": shortages.len(),
    });
    if let (Some(map), Value::Object(extra)) = (payload.as_object_mut(), how.extra) {
        map.extend(extra);
    }
    audit::append(
        tx,
        &ctx.actor()
            .event(how.action, "document", Some(&document.id), payload),
    )?;
    Ok(ValidatedDocument {
        document,
        settlement: Some(settlement),
        shortages,
        customer_balance,
        mentions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::drafts::tests::{cart, fixture, input, save};
    use crate::billing::state::{DocState, state};
    use crate::billing::validate::{ValidateInput, validate};
    use crate::customers::credit::{CollectInput, collect};
    use crate::money::{Money, Qty};
    use crate::sales::cart::CartLine;
    use crate::testkit::{Shop, shop};

    fn invoice_of(shop: &Shop, customer: &Customer, cement: &str, qty: i64) -> Document {
        save(
            shop,
            None,
            input(
                DocType::Invoice,
                cart(Some(customer), vec![CartLine::product(cement, Qty(qty))]),
            ),
        )
        .unwrap()
    }

    fn validate_with(
        shop: &Shop,
        ctx: TillContext,
        id: &str,
        payments: Vec<PaymentInput>,
    ) -> Result<ValidatedDocument, AppError> {
        let id = id.to_owned();
        shop.db
            .write(move |tx| validate(tx, &ctx, ValidateInput { id, payments }))
    }

    fn credit(amount: i64) -> PaymentInput {
        PaymentInput {
            method: PaymentMethod::Credit,
            amount: Money(amount),
            reference: None,
            cheque_bank: None,
            cheque_date: None,
        }
    }

    #[test]
    fn an_invoice_paid_in_cash_pays_the_duty_on_the_cash_part_and_fills_the_drawer() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        shop.open_till(1_000_000);
        let draft = invoice_of(&shop, &amine, &cement.id, 10_000);
        let (ctx, id) = (shop.owner_till(), draft.id.clone());
        let preview = shop
            .db
            .read(move |c| payment_preview(c, &ctx, &id, &[PaymentInput::cash(Money(1_210_000))]))
            .unwrap();
        let done = validate_with(
            &shop,
            shop.owner_till(),
            &draft.id,
            vec![PaymentInput::cash(Money(1_210_000))],
        )
        .unwrap();
        let doc = &done.document;
        assert_eq!(doc.number.as_deref(), Some("FA2026-000001"));
        assert_eq!(
            (
                doc.total_ttc,
                doc.stamp_duty,
                doc.total_due,
                doc.change_given
            ),
            (
                Money(1_190_000),
                Money(11_900),
                Money(1_201_900),
                Money(8_100)
            ),
            "11 900.00 DA in cash: 119 tranches of 1 DA"
        );
        assert_eq!(
            done.settlement.as_ref(),
            Some(&preview),
            "the dialog showed what was booked"
        );
        assert_eq!(
            doc.payments
                .iter()
                .map(|p| (p.method, p.amount))
                .collect::<Vec<_>>(),
            [
                (PaymentMethod::Cash, Money(1_210_000)),
                (PaymentMethod::Change, Money(-8_100))
            ]
        );
        assert_eq!(
            shop.count("SELECT SUM(amount) FROM cash_movements WHERE kind = 'sale'"),
            1_201_900
        );
        assert_eq!(
            shop.count("SELECT SUM(qty_delta) FROM stock_movements WHERE reason = 'sale'"),
            -10_000
        );
        let id = doc.id.clone();
        assert_eq!(
            shop.db.read(move |c| state(c, &id, "2026-09-28")).unwrap(),
            DocState::Paid
        );
    }

    #[test]
    fn a_cash_part_pays_the_duty_and_the_credit_part_goes_to_the_account() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        shop.open_till(0);
        let draft = invoice_of(&shop, &amine, &cement.id, 10_000);
        let done = validate_with(
            &shop,
            shop.owner_till(),
            &draft.id,
            vec![PaymentInput::cash(Money(404_000)), credit(790_000)],
        )
        .unwrap();
        let doc = &done.document;
        assert_eq!(
            (doc.stamp_duty, doc.total_due, doc.credit_amount),
            (Money(4_000), Money(1_194_000), Money(790_000)),
            "duty on the 4 000.00 DA paid in cash only"
        );
        assert_eq!(done.customer_balance, Some(Money(790_000)));
        assert_eq!(doc.due_date.as_deref(), Some("2026-10-28"));
        assert_eq!(
            shop.count("SELECT SUM(amount) FROM cash_movements WHERE kind = 'sale'"),
            404_000
        );
        let id = doc.id.clone();
        assert_eq!(
            shop.db.read(move |c| state(c, &id, "2026-11-02")).unwrap(),
            DocState::Unpaid {
                remaining: Money(790_000),
                due_date: Some("2026-10-28".into()),
                overdue: true
            }
        );
    }

    #[test]
    fn an_invoice_on_account_needs_no_till_and_is_paid_by_a_versement() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        let draft = invoice_of(&shop, &amine, &cement.id, 1_000);
        let doc = validate_with(&shop, shop.owner_till(), &draft.id, vec![credit(119_000)])
            .unwrap()
            .document;
        assert_eq!(
            (doc.stamp_duty, doc.cash_session_id.clone()),
            (Money::ZERO, None)
        );
        let ctx = shop.owner_till();
        let customer_id = amine.id.clone();
        shop.db
            .write(move |tx| {
                collect(
                    tx,
                    &ctx,
                    CollectInput {
                        customer_id,
                        amount: Money(119_000),
                        method: PaymentMethod::CardCib,
                        reference: Some("TPE-7788".into()),
                        cheque_bank: None,
                        cheque_date: None,
                        allocate_to: Vec::new(),
                        note: None,
                    },
                )
                .map(drop)
            })
            .unwrap();
        let id = doc.id.clone();
        assert_eq!(
            shop.db.read(move |c| state(c, &id, "2026-09-28")).unwrap(),
            DocState::Paid
        );
    }

    #[test]
    fn an_invoice_cannot_be_validated_without_the_mandatory_mentions() {
        let shop = shop();
        let amine = shop.b2b_customer("SARL Amine");
        let cement = shop.product("Ciment 25 kg", Money(119_000), &shop.tva19);
        let draft = invoice_of(&shop, &amine, &cement.id, 1_000);
        let err =
            validate_with(&shop, shop.owner_till(), &draft.id, vec![credit(119_000)]).unwrap_err();
        assert_eq!(err.code(), "mentions_missing");
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM counters WHERE doc_type = 'invoice'"),
            0
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM documents WHERE status = 'draft'"),
            1
        );
    }

    #[test]
    fn a_cash_invoice_needs_an_open_till_and_a_credit_above_the_limit_a_manager() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        let draft = invoice_of(&shop, &amine, &cement.id, 1_000);
        let err = validate_with(
            &shop,
            shop.owner_till(),
            &draft.id,
            vec![PaymentInput::cash(Money(121_000))],
        )
        .unwrap_err();
        assert_eq!(
            err.code(),
            "cash_session_closed",
            "1 190.00 + 12.00 of duty is covered, but no till is open"
        );
        let (actor, pack, id) = (shop.owner.actor(), shop.pack.clone(), amine.id.clone());
        shop.db
            .write(move |tx| {
                let mut c = crate::customers::CustomerInput::named("SARL Amine");
                c.customer_type = crate::customers::CustomerType::Reseller;
                c.address = Some("5 rue Didouche Mourad, Alger".into());
                c.nif = Some("000016001234567".into());
                c.rc = Some("16/00-7654321B26".into());
                c.ai = Some("16098765432".into()); // a reseller is identified (M2 CRM-006)
                c.credit_limit = Some(Money(50_000));
                crate::customers::update(tx, &actor, &pack, &id, c).map(drop)
            })
            .unwrap();
        let err = validate_with(&shop, shop.till(), &draft.id, vec![credit(119_000)]).unwrap_err();
        assert_eq!(
            err.code(),
            "credit_limit_exceeded",
            "the cashier has no credit.limit_override"
        );
    }

    #[test]
    fn a_due_date_passed_since_the_draft_was_saved_is_refused_before_numbering() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        let mut typed = input(
            DocType::Invoice,
            cart(
                Some(&amine),
                vec![CartLine::product(&cement.id, Qty(1_000))],
            ),
        );
        typed.due_date = Some("2026-09-30".into());
        let draft = save(&shop, None, typed).unwrap();
        let three_days_later = TillContext::new(
            shop.owner.clone(),
            shop.pack.clone(),
            crate::testkit::NOON + 3 * 86_400_000,
        );
        let err =
            validate_with(&shop, three_days_later, &draft.id, vec![credit(119_000)]).unwrap_err();
        assert_eq!(
            (err.payload().field.unwrap_or_default(), err.code()),
            ("dueDate".into(), "date_invalid".into()),
            "a 'date de règlement' before the invoice date is never numbered"
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM counters WHERE doc_type = 'invoice'"),
            0
        );
        let done =
            validate_with(&shop, shop.owner_till(), &draft.id, vec![credit(119_000)]).unwrap();
        assert_eq!(done.document.due_date.as_deref(), Some("2026-09-30"));
    }

    fn set_exempt(shop: &Shop, customer: &Customer, exempt: bool) {
        let (actor, pack, current) = (shop.owner.actor(), shop.pack.clone(), customer.clone());
        shop.db
            .write(move |tx| {
                let mut c = crate::customers::CustomerInput::from(&current);
                c.vat_exempt = exempt;
                c.vat_exempt_ref = exempt.then(|| "ATT-2026-114".to_owned());
                crate::customers::update(tx, &actor, &pack, &current.id, c).map(drop)
            })
            .unwrap();
    }

    #[test]
    fn an_invoice_is_numbered_only_in_the_tax_context_it_was_priced_in() {
        use crate::billing::drafts::tests::exempt_customer;
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        let code = |id: &str| {
            let total = shop
                .db
                .read({
                    let id = id.to_owned();
                    move |c| documents::get(c, &id)
                })
                .unwrap()
                .total_ttc;
            let transfer =
                PaymentInput::electronic(PaymentMethod::Transfer, total, Some("VIR-2026-0931"));
            validate_with(&shop, shop.owner_till(), id, vec![transfer])
                .map(drop)
                .map_err(|e| e.code())
        };
        // Priced at 0 % for an exempt buyer whose exemption has ended since.
        let state = exempt_customer(&shop);
        let zero_rated = invoice_of(&shop, &state, &cement.id, 1_000);
        assert_eq!(zero_rated.tax_total, Money::ZERO);
        set_exempt(&shop, &state, false);
        assert_eq!(
            code(&zero_rated.id),
            Err("document_tax_context_changed".into())
        );
        // Priced with TVA for a buyer who has become exempt since.
        let with_tva = invoice_of(&shop, &amine, &cement.id, 1_000);
        set_exempt(&shop, &amine, true);
        assert_eq!(
            code(&with_tva.id),
            Err("document_tax_context_changed".into())
        );
        // Saved again, each is priced in today's context and numbered.
        let state_now = shop.db.read({
            let id = state.id.clone();
            move |c| crate::customers::get(c, &id)
        });
        let state_now = state_now.unwrap();
        let again = save(
            &shop,
            Some(&zero_rated.id),
            input(
                DocType::Invoice,
                cart(
                    Some(&state_now),
                    vec![CartLine::product(&cement.id, Qty(1_000))],
                ),
            ),
        )
        .unwrap();
        assert_eq!(again.tax_total, Money(19_000));
        assert_eq!(code(&again.id), Ok(()));
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM counters WHERE doc_type = 'invoice'"),
            1
        );
        // The shop's regime changed since the draft was priced (CTCA art. 64: no TVA under IFU).
        let other = shop.b2b_customer("EURL Bâtir");
        let reel = invoice_of(&shop, &other, &cement.id, 1_000);
        let pack = shop.pack.clone();
        shop.db
            .write(move |tx| {
                let mut c = crate::company::CompanyInput::from(
                    crate::company::get(tx)?.ok_or_else(|| AppError::conflict("setup_required"))?,
                );
                c.tax_regime = TaxRegime::Ifu;
                crate::company::save(tx, &crate::audit::Actor::system(None), &pack, c).map(drop)
            })
            .unwrap();
        assert_eq!(code(&reel.id), Err("document_tax_context_changed".into()));
    }

    #[test]
    fn the_payment_preview_is_for_invoice_drafts_only() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        let quote = save(
            &shop,
            None,
            input(
                DocType::Quote,
                cart(
                    Some(&amine),
                    vec![CartLine::product(&cement.id, Qty(1_000))],
                ),
            ),
        )
        .unwrap();
        let preview_of = |id: &str| {
            let (ctx, id) = (shop.owner_till(), id.to_owned());
            shop.db
                .read(move |c| payment_preview(c, &ctx, &id, &[]))
                .map_err(|e| e.code())
        };
        assert_eq!(preview_of(&quote.id), Err("document_kind_invalid".into()));
        let draft = invoice_of(&shop, &amine, &cement.id, 1_000);
        assert_eq!(
            preview_of(&draft.id).map(|s| s.missing),
            Ok(Money(120_200)),
            "no tender yet: the whole amount in cash, 12.00 of duty"
        );
        validate_with(&shop, shop.owner_till(), &draft.id, vec![credit(119_000)]).unwrap();
        assert_eq!(
            preview_of(&draft.id),
            Err("document_already_validated".into())
        );
    }
}
