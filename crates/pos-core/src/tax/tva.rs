//! TVA on document lines (spec §6), in the company's price mode:
//!
//! - TTC mode (retail): `gross = round(unit_price × qty)`, `discount = round(gross × bp)` or a
//!   fixed amount, `total_ttc = gross − discount`,
//!   `total_ht = round(total_ttc × 10000 / (10000 + rate))`, `tax = total_ttc − total_ht`.
//! - HT mode (wholesale): `total_ht = round(unit_price × qty) − discount`,
//!   `tax = round(total_ht × rate)`, `total_ttc = total_ht + tax`.
//!
//! Every rounding is once, half away from zero, to the centime. Document totals are sums of
//! line totals and per-rate totals are sums of line taxes: nothing is rounded again.

use crate::company::PriceMode;
use crate::error::AppError;
use crate::money::{BP_SCALE, Money, Qty, Rate};
use serde::{Deserialize, Serialize};
use specta::Type;

/// A line discount: a percentage of the gross amount, or a fixed amount (PRM-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Discount {
    #[default]
    None,
    Percent(Rate),
    Amount(Money),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineInput {
    pub unit_price: Money,
    pub qty: Qty,
    pub discount: Discount,
    pub rate: Rate,
}

/// The amounts of one line. `gross` is in the price mode (TTC or HT).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LineAmounts {
    pub gross: Money,
    pub discount_bp: Rate,
    pub discount_amount: Money,
    pub total_ht: Money,
    pub tax: Money,
    pub total_ttc: Money,
}

/// Per-rate totals for the document footer and the TVA reports (`document_taxes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RateTotal {
    pub rate_bp: Rate,
    pub base_ht: Money,
    pub tax: Money,
    pub total_ttc: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    /// The lines' gross before discount, in the price mode (TTC or HT): the subtotal a till
    /// shows above the discount (spec §10.5). `gross_total − discount_total` is `total_ttc` in
    /// TTC mode and `subtotal_ht` in HT mode.
    pub gross_total: Money,
    pub subtotal_ht: Money,
    pub discount_total: Money,
    pub tax_total: Money,
    pub total_ttc: Money,
    /// Highest rate first.
    pub per_rate: Vec<RateTotal>,
}

fn too_large() -> AppError {
    AppError::validation("amount", "amount_invalid")
}

/// Computes one line. Quantities are positive (returns negate a computed line); a discount lies
/// within 0..=100 % or 0..=gross (`discount_invalid`).
pub fn compute_line(mode: PriceMode, line: &LineInput) -> Result<LineAmounts, AppError> {
    if !line.qty.is_positive() {
        return Err(AppError::validation("qty", "qty_invalid"));
    }
    if line.unit_price.is_negative() {
        return Err(AppError::validation("unitPrice", "amount_invalid"));
    }
    if !line.rate.is_fraction() {
        return Err(AppError::validation("rate", "value_invalid"));
    }
    let gross = line.unit_price.mul_qty(line.qty).ok_or_else(too_large)?;
    let (discount_bp, discount_amount) = match line.discount {
        Discount::None => (Rate::ZERO, Money::ZERO),
        Discount::Percent(bp) if bp.is_fraction() => {
            (bp, gross.apply_rate(bp).ok_or_else(too_large)?)
        }
        Discount::Amount(amount) if !amount.is_negative() && amount <= gross => {
            (Rate::ZERO, amount)
        }
        Discount::Percent(_) | Discount::Amount(_) => {
            return Err(AppError::validation("discount", "discount_invalid"));
        }
    };
    let net = gross.checked_sub(discount_amount).ok_or_else(too_large)?;
    let rate = i64::from(line.rate.0);
    let (total_ht, tax, total_ttc) = match mode {
        PriceMode::Ttc => {
            let ht = net
                .mul_div(BP_SCALE, BP_SCALE + rate)
                .ok_or_else(too_large)?;
            (ht, net.checked_sub(ht).ok_or_else(too_large)?, net)
        }
        PriceMode::Ht => {
            let tax = net.apply_rate(line.rate).ok_or_else(too_large)?;
            (net, tax, net.checked_add(tax).ok_or_else(too_large)?)
        }
    };
    Ok(LineAmounts {
        gross,
        discount_bp,
        discount_amount,
        total_ht,
        tax,
        total_ttc,
    })
}

/// One unit at `unit_price` and `rate` as the till charges it in `mode`, TTC: the consumer price
/// a regulated ceiling is held to (PRC-012). The price itself in TTC mode; in HT mode the HT
/// price plus its TVA, rounded once as on a line.
pub fn unit_ttc(mode: PriceMode, unit_price: Money, rate: Rate) -> Result<Money, AppError> {
    let line = LineInput {
        unit_price,
        qty: Qty::ONE,
        discount: Discount::None,
        rate,
    };
    Ok(compute_line(mode, &line)?.total_ttc)
}

/// The highest HT unit price whose TTC ([`unit_ttc`] in HT mode) does not exceed `ttc`: a
/// consumer ceiling turned into the price of an HT shop that never charges more than it.
pub fn ht_within_ttc(ttc: Money, rate: Rate) -> Result<Money, AppError> {
    if ttc.is_negative() {
        return Err(too_large());
    }
    if !rate.is_fraction() {
        return Err(AppError::validation("rate", "value_invalid"));
    }
    let fits =
        |ht: Money| -> Result<bool, AppError> { Ok(unit_ttc(PriceMode::Ht, ht, rate)? <= ttc) };
    let step = Money(1);
    // The rounded quotient is within a centime of the answer; the TTC grows with the HT price.
    let mut ht = ttc
        .mul_div(BP_SCALE, BP_SCALE + i64::from(rate.0))
        .ok_or_else(too_large)?;
    while ht.is_positive() && !fits(ht)? {
        ht = ht.checked_sub(step).ok_or_else(too_large)?;
    }
    while fits(ht.checked_add(step).ok_or_else(too_large)?)? {
        ht = ht.checked_add(step).ok_or_else(too_large)?;
    }
    Ok(ht)
}

impl LineAmounts {
    /// The same line with every amount negated (a returned quantity).
    pub fn negated(&self) -> Result<LineAmounts, AppError> {
        let neg = |m: Money| m.checked_neg().ok_or_else(too_large);
        Ok(LineAmounts {
            gross: neg(self.gross)?,
            discount_bp: self.discount_bp,
            discount_amount: neg(self.discount_amount)?,
            total_ht: neg(self.total_ht)?,
            tax: neg(self.tax)?,
            total_ttc: neg(self.total_ttc)?,
        })
    }
}

/// Sums lines (with their rate) into document totals and per-rate totals.
pub fn totals<'a>(
    lines: impl IntoIterator<Item = (Rate, &'a LineAmounts)>,
) -> Result<Totals, AppError> {
    let mut per_rate: Vec<RateTotal> = Vec::new();
    let mut all: Vec<&LineAmounts> = Vec::new();
    for (rate, line) in lines {
        all.push(line);
        let slot = match per_rate.iter_mut().find(|r| r.rate_bp == rate) {
            Some(slot) => slot,
            None => {
                per_rate.push(RateTotal {
                    rate_bp: rate,
                    base_ht: Money::ZERO,
                    tax: Money::ZERO,
                    total_ttc: Money::ZERO,
                });
                per_rate.last_mut().ok_or_else(too_large)?
            }
        };
        slot.base_ht = slot
            .base_ht
            .checked_add(line.total_ht)
            .ok_or_else(too_large)?;
        slot.tax = slot.tax.checked_add(line.tax).ok_or_else(too_large)?;
        slot.total_ttc = slot
            .total_ttc
            .checked_add(line.total_ttc)
            .ok_or_else(too_large)?;
    }
    per_rate.sort_by_key(|r| std::cmp::Reverse(r.rate_bp));
    let sum = |f: fn(&LineAmounts) -> Money| {
        Money::checked_sum(all.iter().map(|l| f(l))).ok_or_else(too_large)
    };
    Ok(Totals {
        gross_total: sum(|l| l.gross)?,
        subtotal_ht: sum(|l| l.total_ht)?,
        discount_total: sum(|l| l.discount_amount)?,
        tax_total: sum(|l| l.tax)?,
        total_ttc: sum(|l| l.total_ttc)?,
        per_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn line(unit_price: i64, qty: i64, discount: Discount, rate: i32) -> LineInput {
        LineInput {
            unit_price: Money(unit_price),
            qty: Qty(qty),
            discount,
            rate: Rate(rate),
        }
    }

    fn ttc(l: LineInput) -> LineAmounts {
        compute_line(PriceMode::Ttc, &l).unwrap()
    }

    #[test]
    fn ttc_mode_extracts_the_tax_from_the_price() {
        let a = ttc(line(11_900, 1_000, Discount::None, 1_900));
        assert_eq!(
            (a.total_ttc, a.total_ht, a.tax),
            (Money(11_900), Money(10_000), Money(1_900))
        );
        // 3 × 100.00 DA at 19 %: 300.00 TTC = 252.10 HT (252.1008…) + 47.90 TVA.
        let a = ttc(line(10_000, 3_000, Discount::None, 1_900));
        assert_eq!(
            (a.gross, a.total_ht, a.tax),
            (Money(30_000), Money(25_210), Money(4_790))
        );
        // Exempt: no tax at all.
        let a = ttc(line(2_500, 2_000, Discount::None, 0));
        assert_eq!(
            (a.total_ht, a.tax, a.total_ttc),
            (Money(5_000), Money::ZERO, Money(5_000))
        );
    }

    #[test]
    fn weighed_quantities_round_once() {
        // 120.00 DA/kg × 0.255 kg = 30.60 DA; at 9 %: 28.07 HT + 2.53.
        let a = ttc(line(12_000, 255, Discount::None, 900));
        assert_eq!(
            (a.total_ttc, a.total_ht, a.tax),
            (Money(3_060), Money(2_807), Money(253))
        );
        // 33.30 DA × 0.5 = 16.65 exactly; 33.33 × 0.5 = 16.665 → 16.67.
        assert_eq!(
            ttc(line(3_333, 500, Discount::None, 0)).total_ttc,
            Money(1_667)
        );
    }

    #[test]
    fn discounts_come_off_the_gross_amount() {
        // 3 × 45.00 = 135.00, 10 % = 13.50 → 121.50 TTC.
        let a = ttc(line(4_500, 3_000, Discount::Percent(Rate(1_000)), 1_900));
        assert_eq!(
            (a.discount_bp, a.discount_amount, a.total_ttc),
            (Rate(1_000), Money(1_350), Money(12_150))
        );
        let a = ttc(line(4_500, 3_000, Discount::Amount(Money(500)), 1_900));
        assert_eq!(
            (a.discount_bp, a.discount_amount, a.total_ttc),
            (Rate::ZERO, Money(500), Money(13_000))
        );
        let full = ttc(line(4_500, 1_000, Discount::Percent(Rate(10_000)), 1_900));
        assert_eq!(
            (full.total_ttc, full.total_ht, full.tax),
            (Money::ZERO, Money::ZERO, Money::ZERO)
        );
        for bad in [
            Discount::Percent(Rate(10_001)),
            Discount::Amount(Money(4_501)),
            Discount::Amount(Money(-1)),
        ] {
            let err = compute_line(PriceMode::Ttc, &line(4_500, 1_000, bad, 1_900)).unwrap_err();
            assert_eq!(
                (err.code(), err.payload().field),
                ("discount_invalid".into(), Some("discount".into()))
            );
        }
    }

    #[test]
    fn ht_mode_adds_the_tax_to_the_price() {
        // 100.00 HT × 1.5 at 9 % = 150.00 + 13.50 = 163.50.
        let a = compute_line(PriceMode::Ht, &line(10_000, 1_500, Discount::None, 900)).unwrap();
        assert_eq!(
            (a.total_ht, a.tax, a.total_ttc),
            (Money(15_000), Money(1_350), Money(16_350))
        );
        // 10 % off 200.00 HT at 19 %: 180.00 + 34.20 = 214.20.
        let a = compute_line(
            PriceMode::Ht,
            &line(20_000, 1_000, Discount::Percent(Rate(1_000)), 1_900),
        )
        .unwrap();
        assert_eq!(
            (a.total_ht, a.tax, a.total_ttc),
            (Money(18_000), Money(3_420), Money(21_420))
        );
    }

    #[test]
    fn bad_inputs_are_refused_not_panicked() {
        let e = compute_line(PriceMode::Ttc, &line(100, 0, Discount::None, 1_900)).unwrap_err();
        assert_eq!(e.code(), "qty_invalid");
        let e = compute_line(PriceMode::Ttc, &line(-1, 1_000, Discount::None, 1_900)).unwrap_err();
        assert_eq!(e.code(), "amount_invalid");
        let e =
            compute_line(PriceMode::Ttc, &line(100, 1_000, Discount::None, 10_001)).unwrap_err();
        assert_eq!(e.code(), "value_invalid");
        let e =
            compute_line(PriceMode::Ht, &line(i64::MAX, 2_000, Discount::None, 1_900)).unwrap_err();
        assert_eq!(e.code(), "amount_invalid");
    }

    #[test]
    fn document_totals_sum_lines_without_rounding_again() {
        // Two 10.00 DA lines at 19 %: 8.40 + 1.60 each. The tax of the sum would be 3.19; the
        // document keeps the sum of the line taxes, 3.20 (spec §6).
        let a = ttc(line(1_000, 1_000, Discount::None, 1_900));
        let b = ttc(line(1_000, 1_000, Discount::None, 1_900));
        let c = ttc(line(5_000, 1_000, Discount::Percent(Rate(1_000)), 0));
        let t = totals([(Rate(1_900), &a), (Rate(0), &c), (Rate(1_900), &b)]).unwrap();
        assert_eq!(
            (t.subtotal_ht, t.tax_total, t.total_ttc, t.discount_total),
            (Money(6_180), Money(320), Money(6_500), Money(500))
        );
        assert_eq!(
            t.per_rate,
            [
                RateTotal {
                    rate_bp: Rate(1_900),
                    base_ht: Money(1_680),
                    tax: Money(320),
                    total_ttc: Money(2_000)
                },
                RateTotal {
                    rate_bp: Rate(0),
                    base_ht: Money(4_500),
                    tax: Money::ZERO,
                    total_ttc: Money(4_500)
                },
            ]
        );
        let neg = a.negated().unwrap();
        assert_eq!(
            (neg.total_ttc, neg.tax, neg.discount_bp),
            (Money(-1_000), Money(-160), Rate::ZERO)
        );
        assert_eq!(totals([]).unwrap(), Totals::default());
    }

    #[test]
    fn the_subtotal_is_the_gross_before_discount() {
        // The till shows subtotal − discount: 20.00 + 50.00 − 5.00 = 65.00 TTC.
        let a = ttc(line(1_000, 2_000, Discount::None, 1_900));
        let c = ttc(line(5_000, 1_000, Discount::Percent(Rate(1_000)), 0));
        let t = totals([(Rate(1_900), &a), (Rate(0), &c)]).unwrap();
        assert_eq!(
            (t.gross_total, t.discount_total, t.total_ttc),
            (Money(7_000), Money(500), Money(6_500))
        );
        // HT mode: the gross is HT, subtotal − discount is the HT total and the tax comes on top.
        let h = compute_line(
            PriceMode::Ht,
            &line(20_000, 1_000, Discount::Percent(Rate(1_000)), 1_900),
        )
        .unwrap();
        let t = totals([(Rate(1_900), &h)]).unwrap();
        assert_eq!(
            (t.gross_total, t.discount_total, t.subtotal_ht, t.total_ttc),
            (Money(20_000), Money(2_000), Money(18_000), Money(21_420))
        );
    }

    #[test]
    fn one_unit_ttc_and_the_ht_price_that_keeps_within_a_ttc_ceiling() {
        // PRC-012: a regulated ceiling is a consumer price TTC for one unit.
        assert_eq!(
            unit_ttc(PriceMode::Ht, Money(62_000), Rate(900)).unwrap(),
            Money(67_580)
        );
        assert_eq!(
            unit_ttc(PriceMode::Ttc, Money(65_000), Rate(900)).unwrap(),
            Money(65_000)
        );
        // 650.00 TTC at 9 %: 596.33 HT + 53.67 (53.6697) = 650.00; 596.34 would be 650.01.
        assert_eq!(
            ht_within_ttc(Money(65_000), Rate(900)).unwrap(),
            Money(59_633)
        );
        assert_eq!(
            ht_within_ttc(Money(12_500), Rate(900)).unwrap(),
            Money(11_468)
        );
        assert_eq!(
            ht_within_ttc(Money(2_500), Rate::ZERO).unwrap(),
            Money(2_500)
        );
        assert_eq!(
            ht_within_ttc(Money::ZERO, Rate(1_900)).unwrap(),
            Money::ZERO
        );
        let e = ht_within_ttc(Money(-1), Rate(900)).unwrap_err();
        assert_eq!(e.code(), "amount_invalid");
        let e = ht_within_ttc(Money(100), Rate(10_001)).unwrap_err();
        assert_eq!(e.code(), "value_invalid");
    }

    proptest! {
        #[test]
        fn the_ht_price_under_a_ceiling_is_the_highest_that_keeps_within_it(
            ceiling in 0i64..10_000_000_000,
            rate in 0i32..=10_000,
        ) {
            let (ceiling, rate) = (Money(ceiling), Rate(rate));
            let ht = ht_within_ttc(ceiling, rate).unwrap();
            prop_assert!(!ht.is_negative());
            prop_assert!(unit_ttc(PriceMode::Ht, ht, rate).unwrap() <= ceiling);
            let next = ht.checked_add(Money(1)).unwrap();
            prop_assert!(unit_ttc(PriceMode::Ht, next, rate).unwrap() > ceiling);
        }
    }

    proptest! {
        #[test]
        fn the_subtotal_less_the_discount_is_the_net_total(
            lines in prop::collection::vec(
                (0i64..10_000_000, 1i64..100_000, 0i32..=10_000, prop::sample::select(vec![0i32, 900, 1_900])),
                1..8,
            ),
            ht_mode in any::<bool>(),
        ) {
            let mode = if ht_mode { PriceMode::Ht } else { PriceMode::Ttc };
            let computed: Vec<(Rate, LineAmounts)> = lines
                .iter()
                .map(|&(price, qty, bp, rate)| {
                    (Rate(rate), compute_line(mode, &line(price, qty, Discount::Percent(Rate(bp)), rate)).unwrap())
                })
                .collect();
            let t = totals(computed.iter().map(|(rate, a)| (*rate, a))).unwrap();
            let net = if ht_mode { t.subtotal_ht } else { t.total_ttc };
            prop_assert_eq!(t.gross_total.0 - t.discount_total.0, net.0);
        }
    }

    proptest! {
        #[test]
        fn lines_always_balance(
            price in 0i64..100_000_000,
            qty in 1i64..1_000_000,
            rate in prop::sample::select(vec![0i32, 900, 1_900]),
            discount_bp in 0i32..=10_000,
            ht_mode in any::<bool>(),
        ) {
            let mode = if ht_mode { PriceMode::Ht } else { PriceMode::Ttc };
            let l = line(price, qty, Discount::Percent(Rate(discount_bp)), rate);
            let a = compute_line(mode, &l).unwrap();
            prop_assert_eq!(a.total_ht.0 + a.tax.0, a.total_ttc.0);
            prop_assert!(a.discount_amount <= a.gross && !a.discount_amount.is_negative());
            prop_assert!(!a.total_ttc.is_negative() && !a.tax.is_negative());
            if !ht_mode {
                // HT is the TTC divided by (1 + rate), rounded to the nearest centime.
                let scale = i128::from(BP_SCALE) + i128::from(rate);
                let error = (i128::from(a.total_ht.0) * scale - i128::from(a.total_ttc.0) * i128::from(BP_SCALE)).abs();
                prop_assert!(2 * error <= scale, "{a:?}");
            }
        }
    }
}
