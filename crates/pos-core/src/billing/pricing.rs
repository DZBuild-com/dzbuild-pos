//! Prices of a commercial document: the till's pipeline (M1a cart, M2 price levels, quantity
//! breaks, floor prices, promotions and customer discounts, with the operator's rights), then the
//! buyer's VAT exemption (TAX-004, décret 05-468 art. 3: "La TVA n'est pas mentionnée si
//! l'acheteur en est exonéré"): an exempt buyer of a réel shop pays the HT price at rate 0.

use crate::company::{PriceMode, TaxRegime};
use crate::customers;
use crate::documents::DocType;
use crate::error::AppError;
use crate::money::{BP_SCALE, Money, Rate};
use crate::sales::cart::{self, Cart, CartLine, PricedCart, PricedLine};
use crate::tax::tva::{self, Discount, LineInput};
use crate::till::TillContext;
use rusqlite::Connection;
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PricedDocument {
    pub cart: PricedCart,
    /// Every line is HT at rate 0 because the buyer is exempt.
    pub vat_exempt: bool,
}

fn too_large() -> AppError {
    AppError::validation("amount", "amount_invalid")
}

pub fn price_document(
    conn: &Connection,
    ctx: &TillContext,
    doc_type: DocType,
    cart: &Cart,
) -> Result<PricedDocument, AppError> {
    price_offer(conn, ctx, doc_type, cart, &[])
}

/// [`price_document`] of a draft converted at the offered prices: the lines typed as one of
/// `offered` keep their offer's terms ([`cart::price_cart_offered`]).
pub fn price_offer(
    conn: &Connection,
    ctx: &TillContext,
    doc_type: DocType,
    cart: &Cart,
    offered: &[CartLine],
) -> Result<PricedDocument, AppError> {
    let mut priced = cart::price_cart_offered(conn, ctx, cart, offered)?;
    let exempt = priced.tax_regime == TaxRegime::Reel
        && matches!(
            doc_type,
            DocType::Invoice | DocType::Quote | DocType::Proforma | DocType::DeliveryNote
        )
        && match &cart.customer_id {
            Some(id) => customers::get(conn, id)?.vat_exempt,
            None => false,
        };
    if exempt {
        let mode = priced.price_mode;
        for line in &mut priced.lines {
            exempt_line(mode, line)?;
        }
        priced.totals = tva::totals(priced.lines.iter().map(|l| (l.tax_rate_bp, &l.amounts)))?;
    }
    Ok(PricedDocument {
        cart: priced,
        vat_exempt: exempt,
    })
}

/// The line without TVA: in a TTC shop the unit price, the catalogue price and a fixed discount
/// lose their TVA (rounded once each); the line is then computed in HT mode at 0 %.
fn exempt_line(mode: PriceMode, line: &mut PricedLine) -> Result<(), AppError> {
    let rate = i64::from(line.tax_rate_bp.0);
    let ht = |m: Money| -> Result<Money, AppError> {
        match mode {
            PriceMode::Ttc => m.mul_div(BP_SCALE, BP_SCALE + rate).ok_or_else(too_large),
            PriceMode::Ht => Ok(m),
        }
    };
    let unit_price = ht(line.unit_price)?;
    let gross = unit_price.mul_qty(line.qty).ok_or_else(too_large)?;
    let discount = if line.amounts.discount_bp.0 > 0 {
        Discount::Percent(line.amounts.discount_bp)
    } else if line.amounts.discount_amount.is_positive() {
        let off = ht(line.amounts.discount_amount)?;
        Discount::Amount(if off > gross { gross } else { off })
    } else {
        Discount::None
    };
    line.amounts = tva::compute_line(
        PriceMode::Ht,
        &LineInput {
            unit_price,
            qty: line.qty,
            discount,
            rate: Rate::ZERO,
        },
    )?;
    line.catalogue_price = line.catalogue_price.map(ht).transpose()?;
    line.unit_price = unit_price;
    line.tax_rate_bp = Rate::ZERO;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::drafts::tests::{cart, exempt_customer, fixture};
    use crate::money::Qty;
    use crate::sales::cart::CartLine;
    use crate::testkit::shop;

    #[test]
    fn only_the_documents_of_an_exempt_buyer_lose_their_tva() {
        let shop = shop();
        let (_, cement, _) = fixture(&shop);
        let state = exempt_customer(&shop);
        let mut ten = CartLine::product(&cement.id, Qty(2_000));
        ten.discount = Discount::Percent(Rate(1_000));
        let typed = cart(Some(&state), vec![ten]);
        let price = |doc_type: DocType| {
            let (ctx, typed) = (shop.owner_till(), typed.clone());
            shop.db
                .read(move |c| price_document(c, &ctx, doc_type, &typed))
                .unwrap()
        };
        let ticket = price(DocType::Ticket);
        assert!(!ticket.vat_exempt, "a till ticket keeps its TVA");
        assert_eq!(
            (ticket.cart.totals.total_ttc, ticket.cart.totals.tax_total),
            (Money(214_200), Money(34_200))
        );
        let proforma = price(DocType::Proforma);
        assert!(proforma.vat_exempt);
        let line = &proforma.cart.lines[0];
        assert_eq!(
            (line.unit_price, line.catalogue_price, line.tax_rate_bp),
            (Money(100_000), Some(Money(100_000)), Rate::ZERO),
            "1 190.00 TTC is 1 000.00 HT"
        );
        assert_eq!(
            (
                line.amounts.discount_bp,
                line.amounts.discount_amount,
                line.amounts.total_ttc
            ),
            (Rate(1_000), Money(20_000), Money(180_000)),
            "10 % of 2 000.00 HT stays 10 %"
        );
        let totals = &proforma.cart.totals;
        assert_eq!(
            (totals.subtotal_ht, totals.tax_total, totals.total_ttc),
            (Money(180_000), Money::ZERO, Money(180_000))
        );
        assert_eq!(
            totals
                .per_rate
                .iter()
                .map(|r| (r.rate_bp, r.tax))
                .collect::<Vec<_>>(),
            [(Rate::ZERO, Money::ZERO)]
        );
    }
}
