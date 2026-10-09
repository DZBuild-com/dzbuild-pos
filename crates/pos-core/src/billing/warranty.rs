//! Warranty certificates (DOC-012, décret 13-327 art. 6; MVP for phone and electronics shops):
//! which lines of a validated ticket or invoice carry a warranty (the product's
//! `warranty_months`, M3) and with which serial numbers (M2 `serials.sale_line_id`, on the
//! ticket line even when the certificate is made from the ticket's invoice).

use super::validate::{buyer_party, seller_party};
use crate::documents::links::{self, LinkKind};
use crate::documents::{self, DocStatus, DocType, Document, DocumentParty, PartyRole};
use crate::error::AppError;
use crate::money::{Money, Qty};
use rusqlite::Connection;
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WarrantySource {
    pub document_id: String,
    pub doc_type: DocType,
    pub number: String,
    pub business_date: String,
    /// The guarantor: the document's seller snapshot, else the shop today (tickets).
    pub seller: DocumentParty,
    pub buyer: Option<DocumentParty>,
    pub line_id: String,
    pub product: String,
    pub brand: Option<String>,
    /// The serial numbers (IMEI) sold on the line.
    pub serials: Vec<String>,
    pub unit_price_ttc: Money,
    pub qty: Qty,
    pub months: i64,
}

/// The warranted lines of the validated ticket or invoice `document_id` (only `line_id` when
/// given): `warranty_not_set` when none has a warranty, `warranty_document_invalid` for any
/// other document or state.
pub fn sources(
    conn: &Connection,
    document_id: &str,
    line_id: Option<&str>,
) -> Result<Vec<WarrantySource>, AppError> {
    let doc = documents::get(conn, document_id)?;
    if doc.status != DocStatus::Validated
        || !matches!(doc.doc_type, DocType::Ticket | DocType::Invoice)
    {
        return Err(AppError::conflict("warranty_document_invalid"));
    }
    let seller = match doc.parties.iter().find(|p| p.role == PartyRole::Seller) {
        Some(s) => s.clone(),
        None => seller_party(conn, doc.lang, None)?,
    };
    let buyer = match (
        doc.parties.iter().find(|p| p.role == PartyRole::Buyer),
        &doc.customer_id,
    ) {
        (Some(b), _) => Some(b.clone()),
        (None, Some(id)) => Some(buyer_party(&crate::customers::get(conn, id)?)),
        (None, None) => None,
    };
    let number = doc.number.clone().unwrap_or_default();
    let sale_lines = sale_lines(conn, &doc)?;
    let mut out = Vec::new();
    for (line, sale_line) in doc.lines.iter().zip(&sale_lines) {
        if line.parent_line_id.is_some() || line_id.is_some_and(|id| id != line.id) {
            continue;
        }
        let Some(product_id) = &line.product_id else {
            continue;
        };
        let extras = crate::catalog::extras::get(conn, product_id)?;
        let Some(months) = extras.warranty_months else {
            continue;
        };
        let serials = match sale_line {
            Some(sale_line) => conn
                .prepare_cached(
                    "SELECT serial_no FROM serials WHERE sale_line_id = ?1 ORDER BY serial_no",
                )?
                .query_map([sale_line], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?,
            None => Vec::new(),
        };
        out.push(WarrantySource {
            document_id: doc.id.clone(),
            doc_type: doc.doc_type,
            number: number.clone(),
            business_date: doc.business_date.clone(),
            seller: seller.clone(),
            buyer: buyer.clone(),
            line_id: line.id.clone(),
            product: line.description.clone(),
            brand: extras.brand.clone(),
            serials,
            unit_price_ttc: line
                .total_ttc
                .mul_div(1_000, line.qty.0)
                .unwrap_or(line.total_ttc),
            qty: line.qty,
            months,
        });
    }
    if out.is_empty() {
        return Err(AppError::conflict("warranty_not_set"));
    }
    Ok(out)
}

/// For each line of `doc`, the till line its serials were sold on (M2 `link_sale` links them to
/// ticket lines only): a ticket's own line; for an invoice made from tickets (SAL-054), the
/// ticket line at the same position, since `convert::copy_lines` copies the tickets' lines in
/// order under new ids — when it is of the same product. Any other line sold none.
fn sale_lines(conn: &Connection, doc: &Document) -> Result<Vec<Option<String>>, AppError> {
    if doc.doc_type == DocType::Ticket {
        return Ok(doc.lines.iter().map(|l| Some(l.id.clone())).collect());
    }
    let mut ticket_lines = Vec::new();
    for source in links::sources(conn, &doc.id)? {
        if source.link.kind == LinkKind::InvoicedTicket {
            ticket_lines.extend(documents::get(conn, &source.link.from_id)?.lines);
        }
    }
    Ok(doc
        .lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            ticket_lines
                .get(i)
                .filter(|t| t.product_id == line.product_id)
                .map(|t| t.id.clone())
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::convert::{TicketInvoiceInput, invoice_ticket};
    use crate::billing::drafts::tests::{cart, input, save};
    use crate::catalog::extras::{self, ProductExtrasInput};
    use crate::money::Qty;
    use crate::payments::PaymentInput;
    use crate::sales::cart::{Cart, CartLine};
    use crate::sales::{SaleInput, create_sale};
    use crate::stock::serials::{SerialLink, link_sale};
    use crate::stock::{ReceiveLine, SHOP_LOCATION_ID, receive};
    use crate::testkit::shop;

    #[test]
    fn a_warranted_phone_line_gives_its_certificate_source_with_the_imei() {
        let shop = shop();
        shop.complete_company();
        let phone = shop.product("Redmi 13 128 Go", Money(3_500_000), &shop.tva19);
        let cable = shop.product("Câble USB-C", Money(80_000), &shop.tva19);
        let (actor, id) = (shop.owner.actor(), phone.id.clone());
        shop.db
            .write(move |tx| {
                extras::set(
                    tx,
                    &actor,
                    &id,
                    ProductExtrasInput {
                        brand: Some("Xiaomi".into()),
                        warranty_months: Some(12),
                        ..ProductExtrasInput::default()
                    },
                )
            })
            .unwrap();
        shop.stock_in(&phone.id, 5_000);
        shop.stock_in(&cable.id, 5_000);
        shop.open_till(0);
        let ctx = shop.till();
        let input = SaleInput {
            cart: Cart::new(
                crate::lang::Lang::Fr,
                vec![
                    CartLine::product(&phone.id, Qty(1_000)),
                    CartLine::product(&cable.id, Qty(1_000)),
                ],
            ),
            payments: vec![PaymentInput::electronic(
                crate::payments::PaymentMethod::CardCib,
                Money(3_580_000),
                Some("TPE-1"),
            )],
        };
        let ticket = shop
            .db
            .write(move |tx| create_sale(tx, &ctx, input))
            .unwrap()
            .document;
        let line_id = ticket.lines[0].id.clone();
        let (pid, lid) = (phone.id.clone(), line_id.clone());
        shop.db
            .write(move |tx| {
                tx.execute(
                    "INSERT INTO serials (id, product_id, serial_no, status, sale_line_id, created_at, updated_at)
                     VALUES (?1, ?2, '356789104512345', 'sold', ?3, 0, 0)",
                    rusqlite::params![crate::ids::new_id(), pid, lid],
                )?;
                Ok(())
            })
            .unwrap();
        let id = ticket.id.clone();
        let found = shop.db.read(move |c| sources(c, &id, None)).unwrap();
        assert_eq!(found.len(), 1, "the cable has no warranty");
        let w = &found[0];
        assert_eq!(
            (w.product.as_str(), w.brand.as_deref(), w.months),
            ("Redmi 13 128 Go", Some("Xiaomi"), 12)
        );
        assert_eq!(w.serials, ["356789104512345"]);
        assert_eq!(w.seller.rc.as_deref(), Some("16/00-1234567B26"));
        assert_eq!(
            (w.line_id.as_str(), w.unit_price_ttc, w.qty),
            (line_id.as_str(), Money(3_500_000), Qty(1_000))
        );
        let (id, cable_line) = (ticket.id.clone(), ticket.lines[1].id.clone());
        let err = shop
            .db
            .read(move |c| sources(c, &id, Some(&cable_line)))
            .unwrap_err();
        assert_eq!(err.code(), "warranty_not_set");
    }

    #[test]
    fn an_invoice_made_from_a_ticket_certifies_the_imei_sold_at_the_till() {
        let shop = shop();
        shop.complete_company();
        let amine = shop.b2b_customer("SARL Amine");
        let cable = shop.product("Câble USB-C", Money(80_000), &shop.tva19);
        let phone = shop.product("Redmi 13 128 Go", Money(3_500_000), &shop.tva19);
        let (actor, id) = (shop.owner.actor(), phone.id.clone());
        shop.db
            .write(move |tx| {
                extras::set(
                    tx,
                    &actor,
                    &id,
                    ProductExtrasInput {
                        brand: Some("Xiaomi".into()),
                        warranty_months: Some(12),
                        track_serials: true,
                        ..ProductExtrasInput::default()
                    },
                )
            })
            .unwrap();
        let (actor, received) = (
            shop.owner.actor(),
            ReceiveLine {
                product_id: phone.id.clone(),
                location_id: SHOP_LOCATION_ID.into(),
                qty: Qty(2_000),
                unit_cost: Money(2_900_000),
                lot: None,
                serials: vec!["356789104512345".into(), "356789104512352".into()],
                supplier_id: None,
                ref_type: "opening".into(),
                ref_id: "rc1".into(),
                note: None,
            },
        );
        shop.db
            .write(move |tx| receive(tx, &actor, &received, 1))
            .unwrap();
        shop.stock_in(&cable.id, 5_000);
        shop.open_till(0);
        // The phone is the ticket's second line: its invoice copies the lines in that order.
        let ctx = shop.till();
        let input = SaleInput {
            cart: Cart::new(
                crate::lang::Lang::Fr,
                vec![
                    CartLine::product(&cable.id, Qty(1_000)),
                    CartLine::product(&phone.id, Qty(1_000)),
                ],
            ),
            payments: vec![PaymentInput::electronic(
                crate::payments::PaymentMethod::CardCib,
                Money(3_580_000),
                Some("TPE-1"),
            )],
        };
        let ticket = shop
            .db
            .write(move |tx| create_sale(tx, &ctx, input))
            .unwrap()
            .document;
        let (actor, ticket_id, sold) = (
            shop.owner.actor(),
            ticket.id.clone(),
            vec![SerialLink {
                line_id: ticket.lines[1].id.clone(),
                serial_no: "356789104512345".into(),
            }],
        );
        shop.db
            .write(move |tx| link_sale(tx, &actor, &ticket_id, sold))
            .unwrap();
        let (ctx, ticket_id, customer_id) = (shop.owner_till(), ticket.id.clone(), amine.id);
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
        let id = invoice.id.clone();
        let found = shop.db.read(move |c| sources(c, &id, None)).unwrap();
        assert_eq!(found.len(), 1, "the cable has no warranty");
        let w = &found[0];
        assert_eq!(
            (w.doc_type, w.number.as_str(), w.line_id.as_str()),
            (
                DocType::Invoice,
                invoice.number.as_deref().unwrap(),
                invoice.lines[1].id.as_str()
            )
        );
        assert_eq!(
            w.serials,
            ["356789104512345"],
            "the IMEI stays linked to the ticket line the invoice copied"
        );
        assert_eq!(
            w.buyer.as_ref().map(|b| b.name.as_str()),
            Some("SARL Amine")
        );
        // The ticket's own certificate still finds it.
        let id = ticket.id.clone();
        let found = shop.db.read(move |c| sources(c, &id, None)).unwrap();
        assert_eq!(found[0].serials, ["356789104512345"]);
    }

    #[test]
    fn a_certificate_is_made_only_from_a_validated_ticket_or_invoice() {
        let shop = shop();
        shop.complete_company();
        let phone = shop.product("Redmi 13 128 Go", Money(3_500_000), &shop.tva19);
        let buyer = shop.consumer("Karim Bensalah");
        let draft = save(
            &shop,
            None,
            input(
                DocType::Invoice,
                cart(Some(&buyer), vec![CartLine::product(&phone.id, Qty(1_000))]),
            ),
        )
        .unwrap();
        let err = shop
            .db
            .read(move |c| sources(c, &draft.id, None))
            .unwrap_err();
        assert_eq!(err.code(), "warranty_document_invalid");
    }
}
