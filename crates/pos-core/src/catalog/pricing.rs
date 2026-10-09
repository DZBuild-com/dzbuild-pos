//! Margin and markup helpers (PRD-060/061) for the product form: the UI never divides money.

use crate::catalog::tax_rates;
use crate::company::{self, PriceMode};
use crate::error::AppError;
use crate::money::{Money, Rate};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PriceHelpInput {
    /// HT cost (CMUP or the typed purchase price).
    pub cost: Money,
    /// The sale price in the company price mode.
    pub price: Option<Money>,
    pub target_margin_bp: Option<Rate>,
    pub tax_rate_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PriceHelp {
    pub price_ht: Option<Money>,
    pub margin: Option<Money>,
    /// Margin over the HT price.
    pub margin_bp: Option<Rate>,
    /// Margin over the cost.
    pub markup_bp: Option<Rate>,
    pub below_cost: bool,
    /// The sale price (company mode) giving `target_margin_bp`.
    pub price_for_target: Option<Money>,
}

fn ratio(num: Money, den: Money) -> Option<Rate> {
    if !den.is_positive() {
        return None;
    }
    num.mul_div(10_000, den.0)
        .and_then(|m| i32::try_from(m.0).ok())
        .map(Rate)
}

pub fn help(conn: &Connection, input: &PriceHelpInput) -> Result<PriceHelp, AppError> {
    let mode = company::get(conn)?
        .ok_or_else(|| AppError::conflict("setup_required"))?
        .price_mode;
    let rate = tax_rates::get(conn, &input.tax_rate_id)?.rate_bp;
    let den = 10_000 + i64::from(rate.0);
    let price_ht = input.price.and_then(|p| match mode {
        PriceMode::Ttc => p.mul_div(10_000, den),
        PriceMode::Ht => Some(p),
    });
    let margin = price_ht.and_then(|ht| ht.checked_sub(input.cost));
    let price_for_target = match input.target_margin_bp {
        Some(t) if (0..10_000).contains(&t.0) => input
            .cost
            .mul_div(10_000, 10_000 - i64::from(t.0))
            .and_then(|ht| match mode {
                PriceMode::Ttc => ht.mul_div(den, 10_000),
                PriceMode::Ht => Some(ht),
            }),
        Some(_) => return Err(AppError::validation("targetMarginBp", "value_invalid")),
        None => None,
    };
    Ok(PriceHelp {
        price_ht,
        margin,
        margin_bp: margin.zip(price_ht).and_then(|(m, ht)| ratio(m, ht)),
        markup_bp: margin.and_then(|m| ratio(m, input.cost)),
        below_cost: margin.is_some_and(Money::is_negative),
        price_for_target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::shop;

    #[test]
    fn margin_markup_and_the_price_for_a_target_margin() {
        let shop = shop();
        let input = PriceHelpInput {
            cost: Money(10_000),
            price: Some(Money(14_280)),
            target_margin_bp: Some(Rate(2_500)),
            tax_rate_id: shop.tva19.clone(),
        };
        let h = shop.db.read(move |c| help(c, &input)).unwrap();
        // TTC 142.80 DA at 19 % → 120 DA HT; margin 20 DA = 16.67 % of the price, 20 % of the cost.
        assert_eq!(
            (h.price_ht, h.margin, h.margin_bp, h.markup_bp),
            (
                Some(Money(12_000)),
                Some(Money(2_000)),
                Some(Rate(1_667)),
                Some(Rate(2_000))
            )
        );
        assert!(!h.below_cost);
        // 25 % margin: 133.33 DA HT (rounded to 133.33) → 158.66 DA TTC.
        assert_eq!(h.price_for_target, Some(Money(15_866)));
        let under = PriceHelpInput {
            cost: Money(10_000),
            price: Some(Money(11_000)),
            target_margin_bp: None,
            tax_rate_id: shop.tva19.clone(),
        };
        assert!(shop.db.read(move |c| help(c, &under)).unwrap().below_cost);
    }
}
