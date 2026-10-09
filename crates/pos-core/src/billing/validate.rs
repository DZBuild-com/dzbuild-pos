//! Numbering a draft (spec §5.2): quotes and proformas (no stock, no cash, a validity date),
//! delivery notes (the stock leaves), invoices (Task 6). Before its number the draft is re-dated
//! to today and frozen: seller and buyer snapshots, second-language names, today's CMUP.

use super::drafts::add_days;
use super::mentions::{self, MentionCheck};
use super::settings::{print_settings, quote_validity_days};
use crate::audit;
use crate::catalog::products::{self, ProductKind};
use crate::company;
use crate::customers::{self, Customer};
use crate::documents::{self, DocStatus, DocType, Document, DocumentParty, PartyRole, parties};
use crate::error::AppError;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::{PaymentInput, Settlement};
use crate::stock::{self, MovementReason, NewMovement, Shortage, StockNeed};
use crate::till::TillContext;
use rusqlite::{Connection, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ValidateInput {
    pub id: String,
    /// Invoices only: the tenders, a `credit` one for the part left on the account.
    #[serde(default)]
    pub payments: Vec<PaymentInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ValidatedDocument {
    pub document: Document,
    pub settlement: Option<Settlement>,
    /// Stock shortages under the `warn` policy.
    pub shortages: Vec<Shortage>,
    /// The customer's balance after an invoice with a credit part.
    pub customer_balance: Option<Money>,
    /// The mentions still missing (warnings on quotes, proformas and delivery notes).
    pub mentions: MentionCheck,
}

impl ValidatedDocument {
    /// The cash an invoice kept in the drawer that took it (a host service's answer: the till
    /// records it, [`crate::cash::record_share`]); `None` for a document without cash.
    pub fn drawer_share(&self) -> Result<Option<crate::cash::DrawerShare>, AppError> {
        match &self.settlement {
            Some(settlement) => {
                crate::cash::DrawerShare::of_settlement(&self.document.id, settlement)
            }
            None => Ok(None),
        }
    }
}

pub fn validate(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: ValidateInput,
) -> Result<ValidatedDocument, AppError> {
    let doc = documents::get(tx, &input.id)?;
    if doc.status != DocStatus::Draft {
        return Err(AppError::conflict("document_already_validated"));
    }
    match doc.doc_type {
        DocType::Quote | DocType::Proforma => validate_offer(tx, ctx, doc, &input.payments),
        DocType::DeliveryNote => validate_delivery(tx, ctx, doc, &input.payments),
        DocType::Invoice => super::invoices::validate_invoice(tx, ctx, doc, input.payments),
        _ => Err(AppError::validation("id", "document_kind_invalid")),
    }
}

fn no_payments(payments: &[PaymentInput]) -> Result<(), AppError> {
    if payments.is_empty() {
        Ok(())
    } else {
        Err(AppError::validation("payments", "value_invalid"))
    }
}

/// A three-language name in `lang`, else French, Arabic, English (spec §5 fallback).
pub(super) fn in_lang(lang: Lang, ar: &str, fr: &str, en: &str) -> String {
    let preferred = match lang {
        Lang::Ar => ar,
        Lang::Fr => fr,
        Lang::En => en,
    };
    [preferred, fr, ar, en]
        .into_iter()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_owned()
}

/// The shop as it is now, in the document language (and the second one). Public: the A4
/// statements of Task 18 print it as their seller block.
pub fn seller_party(
    conn: &Connection,
    lang: Lang,
    second: Option<Lang>,
) -> Result<DocumentParty, AppError> {
    let c = company::get(conn)?.ok_or_else(|| AppError::conflict("setup_required"))?;
    let settings = print_settings(conn)?;
    let name = in_lang(lang, &c.name_ar, &c.name_fr, &c.name_en);
    let activity = settings.activity.in_lang(lang).map(str::to_owned);
    let nin: String = crate::settings::get(conn, "company.nin")?;
    Ok(DocumentParty {
        role: PartyRole::Seller,
        party_id: None,
        name_alt: second
            .map(|l| in_lang(l, &c.name_ar, &c.name_fr, &c.name_en))
            .filter(|n| !n.is_empty() && *n != name),
        activity_alt: second
            .and_then(|l| settings.activity.in_lang(l))
            .map(str::to_owned)
            .filter(|a| Some(a) != activity.as_ref()),
        name,
        activity,
        legal_form: c.legal_form,
        capital: c.capital,
        address: c.address,
        wilaya: c.wilaya,
        phone: c.phone,
        email: c.email,
        rc: c.rc,
        ae_card: settings.ae_card,
        nif: c.nif,
        nis: c.nis,
        ai: c.ai,
        nin: Some(nin).filter(|n| !n.is_empty()),
        rib: c.rib,
        vat_exempt_ref: None,
    })
}

pub(crate) fn buyer_party(c: &Customer) -> DocumentParty {
    DocumentParty {
        role: PartyRole::Buyer,
        party_id: Some(c.id.clone()),
        name: c.name.clone(),
        name_alt: None,
        legal_form: c.legal_form.clone(),
        activity: c.activity.clone(),
        activity_alt: None,
        capital: None,
        address: c.address.clone(),
        wilaya: c.wilaya.clone(),
        phone: c.phone.clone(),
        email: c.email.clone(),
        rc: c.rc.clone(),
        ae_card: None,
        nif: c.nif.clone(),
        nis: c.nis.clone(),
        ai: c.ai.clone(),
        nin: c.nin.clone(),
        rib: None,
        vat_exempt_ref: c.vat_exempt.then(|| c.vat_exempt_ref.clone()).flatten(),
    }
}

/// Re-dates the draft to today and freezes what the document will show forever.
pub(crate) fn freeze(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    doc: &Document,
    buyer: Option<&Customer>,
    refresh_costs: bool,
) -> Result<(), AppError> {
    documents::restamp(tx, &doc.id, ctx.business_date(), ctx.now)?;
    parties::record(tx, &doc.id, &seller_party(tx, doc.lang, doc.second_lang)?)?;
    if let Some(c) = buyer {
        parties::record(tx, &doc.id, &buyer_party(c))?;
    }
    let mut names = Vec::new();
    let mut costs = Vec::new();
    for line in &doc.lines {
        let Some(product_id) = &line.product_id else {
            continue;
        };
        let product = products::get(tx, product_id)?;
        if let Some(second) = doc.second_lang {
            let alt = product.name_in(second);
            if !alt.trim().is_empty() && alt != line.description {
                names.push((line.id.clone(), alt.to_owned()));
            }
        }
        if refresh_costs {
            costs.push((line.id.clone(), product.cost_price));
        }
    }
    documents::set_alt_descriptions(tx, &doc.id, &names)?;
    documents::set_line_costs(tx, &doc.id, &costs)
}

/// Takes the stock of a validated document's lines: the component lines of a kit, not the kit.
/// An archived product takes no movement (M2): a draft typed before one of its products was
/// archived is refused (`lines.{i}.productId`/`product_archived`, `i` the cart line, a kit's
/// for its components) and nothing is numbered.
pub(crate) fn take_stock(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    doc: &Document,
) -> Result<Vec<Shortage>, AppError> {
    let location = stock::default_location(tx)?;
    let top_lines: Vec<&str> = doc
        .lines
        .iter()
        .filter(|l| l.parent_line_id.is_none())
        .map(|l| l.id.as_str())
        .collect();
    let mut takes: Vec<(String, Qty, Money)> = Vec::new();
    for line in &doc.lines {
        let Some(product_id) = &line.product_id else {
            continue;
        };
        let product = products::get(tx, product_id)?;
        if product.kind != ProductKind::Composite && product.track_stock {
            if product.archived {
                let top = line.parent_line_id.as_deref().unwrap_or(&line.id);
                let i = top_lines.iter().position(|id| *id == top).unwrap_or(0);
                return Err(AppError::validation(
                    &format!("lines.{i}.productId"),
                    "product_archived",
                ));
            }
            takes.push((product_id.clone(), line.qty, line.cost_price));
        }
    }
    let needs: Vec<StockNeed> = takes
        .iter()
        .map(|(product_id, qty, _)| StockNeed {
            product_id: product_id.clone(),
            location_id: location.id.clone(),
            qty: *qty,
        })
        .collect();
    let shortages = stock::check_available(tx, &needs)?;
    let (actor, today) = (ctx.actor(), ctx.business_day());
    for (product_id, qty, cost) in &takes {
        let movement = NewMovement {
            product_id: product_id.clone(),
            location_id: location.id.clone(),
            qty_delta: qty
                .checked_neg()
                .ok_or_else(|| AppError::validation("qty", "qty_invalid"))?,
            reason: MovementReason::Sale,
            reason_code: None,
            ref_type: Some("document".into()),
            ref_id: Some(doc.id.clone()),
            unit_cost: Some(*cost),
            note: None,
        };
        // FEFO over the lots of a lot-tracked product, one plain movement otherwise (M2).
        stock::lots::take(tx, &actor, &movement, &today, ctx.now)?;
    }
    Ok(shortages)
}

fn validate_offer(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    doc: Document,
    payments: &[PaymentInput],
) -> Result<ValidatedDocument, AppError> {
    no_payments(payments)?;
    let today = ctx.business_day();
    let mut extras = documents::extras_of(&doc);
    extras.valid_until = Some(match extras.valid_until.take() {
        Some(day) if day < today => return Err(AppError::validation("validUntil", "date_invalid")),
        Some(day) => day,
        None => add_days(ctx, quote_validity_days(tx)?)?,
    });
    documents::set_extras(tx, &doc.id, &extras)?;
    let customer = doc
        .customer_id
        .as_deref()
        .map(|id| customers::get(tx, id))
        .transpose()?;
    let mentions = mentions::check(tx, doc.customer_id.as_deref())?;
    let doc = documents::get(tx, &doc.id)?;
    freeze(tx, ctx, &doc, customer.as_ref(), true)?;
    let document = documents::validate(tx, &doc.id, ctx.now)?;
    let action = if document.doc_type == DocType::Quote {
        "quote.validated"
    } else {
        "proforma.validated"
    };
    audit::append(
        tx,
        &ctx.actor().event(
            action,
            "document",
            Some(&document.id),
            json!({
                "number": document.number,
                "totalDue": document.total_due,
                "customerId": document.customer_id,
                "validUntil": document.valid_until,
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

fn validate_delivery(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    doc: Document,
    payments: &[PaymentInput],
) -> Result<ValidatedDocument, AppError> {
    no_payments(payments)?;
    let customer_id = doc
        .customer_id
        .clone()
        .ok_or_else(|| AppError::validation("customerId", "customer_required"))?;
    let customer = customers::get(tx, &customer_id)?;
    let mentions = mentions::check(tx, Some(&customer_id))?;
    freeze(tx, ctx, &doc, Some(&customer), true)?;
    let document = documents::validate(tx, &doc.id, ctx.now)?;
    let shortages = take_stock(tx, ctx, &document)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "delivery_note.validated",
            "document",
            Some(&document.id),
            json!({
                "number": document.number,
                "customerId": document.customer_id,
                "lines": document.lines.len(),
                "shortages": shortages.len(),
            }),
        ),
    )?;
    Ok(ValidatedDocument {
        document,
        settlement: None,
        shortages,
        customer_balance: None,
        mentions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::drafts::tests::{cart, fixture, input, save};
    use crate::billing::state::{DocState, state};
    use crate::sales::cart::CartLine;
    use crate::testkit::{NOON, Shop, shop};

    fn validate_now(
        shop: &Shop,
        id: &str,
        payments: Vec<PaymentInput>,
    ) -> Result<ValidatedDocument, AppError> {
        let (ctx, id) = (shop.owner_till(), id.to_owned());
        shop.db
            .write(move |tx| validate(tx, &ctx, ValidateInput { id, payments }))
    }

    fn state_on(shop: &Shop, id: &str, day: &'static str) -> DocState {
        let id = id.to_owned();
        shop.db.read(move |c| state(c, &id, day)).unwrap()
    }

    #[test]
    fn a_quote_is_numbered_valid_thirty_days_frozen_and_moves_nothing() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        let draft = save(
            &shop,
            None,
            input(
                DocType::Quote,
                cart(
                    Some(&amine),
                    vec![CartLine::product(&cement.id, Qty(10_000))],
                ),
            ),
        )
        .unwrap();
        let movements = shop.count("SELECT COUNT(*) FROM stock_movements");
        let done = validate_now(&shop, &draft.id, Vec::new()).unwrap();
        let doc = done.document;
        assert_eq!(doc.number.as_deref(), Some("DV2026-000001"));
        assert_eq!(doc.valid_until.as_deref(), Some("2026-10-28"));
        assert_eq!(doc.business_date, "2026-09-28");
        assert_eq!(
            doc.parties.iter().map(|p| p.role).collect::<Vec<_>>(),
            [PartyRole::Seller, PartyRole::Buyer]
        );
        assert_eq!(doc.parties[0].rc.as_deref(), Some("16/00-1234567B26"));
        assert_eq!(
            doc.parties[0].activity.as_deref(),
            Some("Commerce de détail en alimentation générale")
        );
        assert_eq!(doc.parties[1].nif.as_deref(), Some("000016001234567"));
        assert!(done.settlement.is_none());
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM stock_movements"),
            movements
        );
        assert_eq!(shop.count("SELECT COUNT(*) FROM cash_movements"), 0);
        assert_eq!(
            state_on(&shop, &doc.id, "2026-10-01"),
            DocState::Open {
                valid_until: Some("2026-10-28".into())
            }
        );
        assert_eq!(
            state_on(&shop, &doc.id, "2026-10-29"),
            DocState::Expired {
                valid_until: "2026-10-28".into()
            }
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'quote.validated'"),
            1
        );
        let err = validate_now(&shop, &doc.id, Vec::new()).unwrap_err();
        assert_eq!(err.code(), "document_already_validated");
    }

    #[test]
    fn a_proforma_has_its_own_series_and_an_offer_takes_no_payment() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        let one = || {
            cart(
                Some(&amine),
                vec![CartLine::product(&cement.id, Qty(1_000))],
            )
        };
        let pf = save(&shop, None, input(DocType::Proforma, one())).unwrap();
        let err =
            validate_now(&shop, &pf.id, vec![PaymentInput::cash(Money(119_000))]).unwrap_err();
        assert_eq!(
            (err.payload().field.as_deref(), err.code().as_str()),
            (Some("payments"), "value_invalid")
        );
        let doc = validate_now(&shop, &pf.id, Vec::new()).unwrap().document;
        assert_eq!(doc.number.as_deref(), Some("PF2026-000001"));
        let dv = save(&shop, None, input(DocType::Quote, one())).unwrap();
        assert_eq!(
            validate_now(&shop, &dv.id, Vec::new())
                .unwrap()
                .document
                .number
                .as_deref(),
            Some("DV2026-000001")
        );
    }

    #[test]
    fn a_delivery_note_takes_its_stock_at_cmup_and_waits_for_its_invoice() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        let labour = shop.product_with({
            let mut p = crate::catalog::products::ProductInput::simple(
                "Livraison",
                crate::catalog::UNIT_PIECE_ID,
                &shop.tva19,
                Money(150_000),
            );
            p.kind = crate::catalog::products::ProductKind::Service;
            p.track_stock = false;
            p
        });
        let draft = save(
            &shop,
            None,
            input(
                DocType::DeliveryNote,
                cart(
                    Some(&amine),
                    vec![
                        CartLine::product(&cement.id, Qty(3_000)),
                        CartLine::product(&labour.id, Qty(1_000)),
                    ],
                ),
            ),
        )
        .unwrap();
        let err = validate_now(&shop, &draft.id, vec![PaymentInput::cash(Money(100))]).unwrap_err();
        assert_eq!(err.code(), "value_invalid");
        let doc = validate_now(&shop, &draft.id, Vec::new()).unwrap().document;
        assert_eq!(doc.number.as_deref(), Some("BL2026-000001"));
        let id = doc.id.clone();
        let moved: Vec<(String, i64, String)> = shop
            .db
            .read(move |c| {
                let mut stmt = c.prepare(
                    "SELECT product_id, qty_delta, reason FROM stock_movements WHERE ref_id = ?1",
                )?;
                let rows = stmt
                    .query_map([&id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        assert_eq!(moved, [(cement.id.clone(), -3_000, "sale".to_owned())]);
        assert_eq!(state_on(&shop, &doc.id, "2026-09-28"), DocState::ToInvoice);
    }

    /// M2: an archived product takes no movement. A delivery note typed before its product was
    /// archived is refused on its line, and nothing is numbered, moved or journaled.
    #[test]
    fn a_draft_whose_product_was_archived_since_is_not_validated() {
        let shop = shop();
        let (amine, cement, sand) = fixture(&shop);
        shop.stock_in(&cement.id, 1_000);
        let draft = save(
            &shop,
            None,
            input(
                DocType::DeliveryNote,
                cart(
                    Some(&amine),
                    vec![
                        CartLine::product(&cement.id, Qty(1_000)),
                        CartLine::product(&sand.id, Qty(1_000)),
                    ],
                ),
            ),
        )
        .unwrap();
        let (actor, id) = (shop.owner.actor(), sand.id.clone());
        shop.db
            .write(move |tx| products::archive(tx, &actor, &id))
            .unwrap();
        let (movements, journal) = (
            shop.count("SELECT COUNT(*) FROM stock_movements"),
            shop.count("SELECT COUNT(*) FROM audit_log"),
        );
        let err = validate_now(&shop, &draft.id, Vec::new()).unwrap_err();
        assert_eq!(
            (err.payload().field.as_deref(), err.code().as_str()),
            (Some("lines.1.productId"), "product_archived")
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM stock_movements"),
            movements
        );
        assert_eq!(shop.count("SELECT COUNT(*) FROM audit_log"), journal);
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM documents WHERE number IS NOT NULL AND doc_type = 'delivery_note'"),
            0
        );
        // Restored, the same draft is numbered and takes its stock.
        let (actor, id) = (shop.owner.actor(), sand.id.clone());
        shop.db
            .write(move |tx| products::restore(tx, &actor, &id))
            .unwrap();
        shop.stock_in(&sand.id, 1_000);
        let doc = validate_now(&shop, &draft.id, Vec::new()).unwrap().document;
        assert_eq!(doc.number.as_deref(), Some("BL2026-000001"));
    }

    #[test]
    fn a_bilingual_quote_keeps_the_second_names() {
        let shop = shop();
        let (amine, _, _) = fixture(&shop);
        let cement = shop.product_with({
            let mut p = crate::catalog::products::ProductInput::simple(
                "Ciment 25 kg",
                crate::catalog::UNIT_PIECE_ID,
                &shop.tva19,
                Money(119_000),
            );
            p.name_ar = "إسمنت 25 كغ".into();
            p
        });
        let mut i = input(
            DocType::Quote,
            cart(
                Some(&amine),
                vec![CartLine::product(&cement.id, Qty(1_000))],
            ),
        );
        i.second_lang = Some(Lang::Ar);
        let draft = save(&shop, None, i).unwrap();
        let doc = validate_now(&shop, &draft.id, Vec::new()).unwrap().document;
        assert_eq!(doc.lines[0].description_alt.as_deref(), Some("إسمنت 25 كغ"));
        assert_eq!(doc.parties[0].name_alt.as_deref(), Some("مغازة الأمل"));
        assert_eq!(
            doc.parties[0].activity_alt.as_deref(),
            Some("تجارة التجزئة للمواد الغذائية العامة")
        );
    }

    #[test]
    fn a_quote_validated_later_takes_the_day_it_is_numbered() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        let draft = save(
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
        let later = TillContext::new(shop.owner.clone(), shop.pack.clone(), NOON + 3 * 86_400_000);
        let id = draft.id.clone();
        let doc = shop
            .db
            .write(move |tx| {
                validate(
                    tx,
                    &later,
                    ValidateInput {
                        id,
                        payments: Vec::new(),
                    },
                )
            })
            .unwrap()
            .document;
        assert_eq!(doc.business_date, "2026-10-01");
        assert_eq!(doc.valid_until.as_deref(), Some("2026-10-31"));
    }
}
