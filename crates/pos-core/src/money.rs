//! Money, quantities and rates as integers (spec §6). No floats anywhere.
//!
//! - [`Money`]: centimes (`i64`), 1 DA = 100.
//! - [`Qty`]: milli-units (`i64`), 1 piece = 1000, 1 kg = 1000 g.
//! - [`Rate`]: basis points (`i32`), 19 % = 1900.
//!
//! Every multiplication rounds exactly once, half away from zero, to the centime.

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fmt;

/// Milli-units per whole unit.
pub const QTY_SCALE: i64 = 1_000;
/// Basis points in 100 %.
pub const BP_SCALE: i64 = 10_000;

/// An amount in centimes.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
#[serde(transparent)]
pub struct Money(pub i64);

/// A quantity in milli-units.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
#[serde(transparent)]
pub struct Qty(pub i64);

/// A rate in basis points.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
#[serde(transparent)]
pub struct Rate(pub i32);

/// Divides `num` by a positive `den`, rounding half away from zero.
fn div_round(num: i128, den: i128) -> i128 {
    let quotient = num / den;
    let remainder = num % den;
    if 2 * remainder.abs() >= den {
        quotient + num.signum()
    } else {
        quotient
    }
}

impl Money {
    pub const ZERO: Money = Money(0);

    pub fn from_dinars(dinars: i64) -> Option<Money> {
        dinars.checked_mul(100).map(Money)
    }

    pub const fn centimes(self) -> i64 {
        self.0
    }

    pub fn checked_add(self, rhs: Money) -> Option<Money> {
        self.0.checked_add(rhs.0).map(Money)
    }

    pub fn checked_sub(self, rhs: Money) -> Option<Money> {
        self.0.checked_sub(rhs.0).map(Money)
    }

    pub fn checked_neg(self) -> Option<Money> {
        self.0.checked_neg().map(Money)
    }

    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// Sum of amounts, accumulated exactly in `i128` so the result does not depend on order;
    /// `None` when the total does not fit in `i64`.
    pub fn checked_sum<I: IntoIterator<Item = Money>>(items: I) -> Option<Money> {
        let total = items
            .into_iter()
            .try_fold(0i128, |acc, m| acc.checked_add(i128::from(m.0)))?;
        i64::try_from(total).ok().map(Money)
    }

    /// `self × qty`, rounded once (half away from zero) to the centime.
    pub fn mul_qty(self, qty: Qty) -> Option<Money> {
        let exact = i128::from(self.0) * i128::from(qty.0);
        i64::try_from(div_round(exact, i128::from(QTY_SCALE)))
            .ok()
            .map(Money)
    }

    /// `self × n` for a whole count (free items, bundles); `None` on overflow.
    pub fn checked_mul_i64(self, n: i64) -> Option<Money> {
        self.0.checked_mul(n).map(Money)
    }

    /// `self × rate`, rounded once (half away from zero) to the centime.
    pub fn apply_rate(self, rate: Rate) -> Option<Money> {
        let exact = i128::from(self.0) * i128::from(rate.0);
        i64::try_from(div_round(exact, i128::from(BP_SCALE)))
            .ok()
            .map(Money)
    }

    /// `self × num / den`, rounded once (half away from zero) to the centime: TTC → HT
    /// (`× 10000 / (10000 + rate)`) and pro-rata refunds. `None` when `den` is not positive or
    /// the result does not fit.
    pub fn mul_div(self, num: i64, den: i64) -> Option<Money> {
        if den <= 0 {
            return None;
        }
        let exact = i128::from(self.0) * i128::from(num);
        i64::try_from(div_round(exact, i128::from(den)))
            .ok()
            .map(Money)
    }

    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    pub fn checked_abs(self) -> Option<Money> {
        self.0.checked_abs().map(Money)
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        write!(f, "{sign}{}.{:02}", abs / 100, abs % 100)
    }
}

impl ToSql for Money {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0))
    }
}

impl FromSql for Money {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value.as_i64().map(Money)
    }
}

impl Qty {
    pub const ZERO: Qty = Qty(0);
    pub const ONE: Qty = Qty(QTY_SCALE);

    pub fn from_units(units: i64) -> Option<Qty> {
        units.checked_mul(QTY_SCALE).map(Qty)
    }

    pub fn checked_add(self, rhs: Qty) -> Option<Qty> {
        self.0.checked_add(rhs.0).map(Qty)
    }

    pub fn checked_sub(self, rhs: Qty) -> Option<Qty> {
        self.0.checked_sub(rhs.0).map(Qty)
    }

    pub fn checked_neg(self) -> Option<Qty> {
        self.0.checked_neg().map(Qty)
    }

    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// `self × other` (a component quantity per unit × the quantity sold), rounded once, half
    /// away from zero, to the milli-unit.
    pub fn mul_qty(self, other: Qty) -> Option<Qty> {
        let exact = i128::from(self.0) * i128::from(other.0);
        i64::try_from(div_round(exact, i128::from(QTY_SCALE)))
            .ok()
            .map(Qty)
    }

    /// True when the quantity needs no more decimals than a unit with `decimals` (0..=3)
    /// allows: 1.250 kg fits 3 decimals, 1.5 pieces does not fit 0.
    pub fn fits_decimals(self, decimals: u8) -> bool {
        let step = 10_i64.pow(3 - u32::from(decimals.min(3)));
        self.0 % step == 0
    }
}

impl ToSql for Qty {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0))
    }
}

impl FromSql for Qty {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value.as_i64().map(Qty)
    }
}

impl Rate {
    pub const ZERO: Rate = Rate(0);
    pub const FULL: Rate = Rate(10_000);

    pub fn from_percent(percent: i32) -> Option<Rate> {
        percent.checked_mul(100).map(Rate)
    }

    /// Discounts and limits must lie in 0..=100 %.
    pub const fn is_fraction(self) -> bool {
        self.0 >= 0 && self.0 <= 10_000
    }
}

impl ToSql for Rate {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0))
    }
}

impl FromSql for Rate {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let raw = value.as_i64()?;
        i32::try_from(raw)
            .map(Rate)
            .map_err(|_| FromSqlError::OutOfRange(raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn mul_qty_rounds_half_away_from_zero_once() {
        assert_eq!(Money(1_050).mul_qty(Qty(1_500)), Some(Money(1_575))); // 10.50 DA x 1.5
        assert_eq!(Money(333).mul_qty(Qty(500)), Some(Money(167))); // 1.665 -> 1.67
        assert_eq!(Money(-333).mul_qty(Qty(500)), Some(Money(-167)));
        assert_eq!(Money(333).mul_qty(Qty(499)), Some(Money(166))); // 1.66167 -> 1.66
        assert_eq!(Money(12_000).mul_qty(Qty(250)), Some(Money(3_000))); // 120 DA/kg x 250 g
    }

    #[test]
    fn apply_rate_rounds_half_away_from_zero_once() {
        assert_eq!(Money(10_000).apply_rate(Rate(1_900)), Some(Money(1_900)));
        assert_eq!(Money(1_005).apply_rate(Rate(1_900)), Some(Money(191))); // 1.9095 -> 1.91
        assert_eq!(Money(-1_005).apply_rate(Rate(1_900)), Some(Money(-191)));
        assert_eq!(Money(50).apply_rate(Rate(100)), Some(Money(1))); // 0.5 centime -> 1
        assert_eq!(Money(-50).apply_rate(Rate(100)), Some(Money(-1))); // -0.5 centime -> -1
    }

    #[test]
    fn overflow_is_none_not_a_panic() {
        assert_eq!(Money(i64::MAX).checked_add(Money(1)), None);
        assert_eq!(Money(i64::MIN).checked_sub(Money(1)), None);
        assert_eq!(Money(i64::MAX).mul_qty(Qty(2_000)), None);
        assert_eq!(Money::checked_sum([Money(i64::MAX), Money(1)]), None);
        assert_eq!(Money::from_dinars(i64::MAX), None);
        assert_eq!(Money(i64::MIN).checked_neg(), None);
        assert_eq!(Qty(i64::MAX).checked_add(Qty(1)), None);
        assert_eq!(Qty(i64::MIN).checked_sub(Qty(1)), None);
        assert_eq!(Qty::from_units(i64::MAX), None);
        assert_eq!(Rate::from_percent(i32::MAX), None);
    }

    #[test]
    fn checked_arithmetic_and_accessors() {
        assert_eq!(Money(450_005).centimes(), 450_005);
        assert_eq!(Money(1_000).checked_sub(Money(1_575)), Some(Money(-575)));
        assert_eq!(Qty(1_500).checked_add(Qty(250)), Some(Qty(1_750)));
        assert_eq!(Qty(1_500).checked_sub(Qty(2_000)), Some(Qty(-500)));
    }

    #[test]
    fn checked_mul_i64_multiplies_or_overflows() {
        assert_eq!(Money(3_000).checked_mul_i64(2), Some(Money(6_000)));
        assert_eq!(Money(-250).checked_mul_i64(4), Some(Money(-1_000)));
        assert_eq!(Money(3_000).checked_mul_i64(0), Some(Money::ZERO));
        assert_eq!(Money(i64::MAX).checked_mul_i64(2), None);
        assert_eq!(Money(i64::MIN).checked_mul_i64(-1), None);
    }

    #[test]
    fn checked_sum_fails_only_when_the_total_overflows() {
        let max_then_back = [Money(i64::MAX), Money(1), Money(-1)];
        assert_eq!(Money::checked_sum(max_then_back), Some(Money(i64::MAX)));
        let min_then_back = [Money(i64::MIN), Money(-1), Money(1)];
        assert_eq!(Money::checked_sum(min_then_back), Some(Money(i64::MIN)));
        assert_eq!(Money::checked_sum([]), Some(Money::ZERO));
    }

    #[test]
    fn constructors_and_flags() {
        assert_eq!(Money::from_dinars(4_500), Some(Money(450_000)));
        assert_eq!(Qty::from_units(3), Some(Qty(3_000)));
        assert_eq!(Rate::from_percent(19), Some(Rate(1_900)));
        assert!(
            Rate(10_000).is_fraction() && !Rate(10_001).is_fraction() && !Rate(-1).is_fraction()
        );
        assert!(Money(-1).is_negative());
    }

    #[test]
    fn serde_is_a_plain_integer() {
        assert_eq!(serde_json::to_string(&Money(450_050)).unwrap(), "450050");
        assert_eq!(serde_json::from_str::<Qty>("1500").unwrap(), Qty(1_500));
        assert_eq!(serde_json::to_string(&Rate(1_900)).unwrap(), "1900");
    }

    #[test]
    fn display_is_dinars_dot_centimes() {
        assert_eq!(Money(450_005).to_string(), "4500.05");
        assert_eq!(Money(-5).to_string(), "-0.05");
    }

    #[test]
    fn sqlite_roundtrip_is_integer() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let (m, kind): (Money, String) = conn
            .query_row("SELECT ?1, typeof(?1)", [Money(123_456)], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(m, Money(123_456));
        assert_eq!(kind, "integer");
    }

    #[test]
    fn qty_and_rate_are_stored_as_integers() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let (q, r, kinds): (Qty, Rate, String) = conn
            .query_row(
                "SELECT ?1, ?2, typeof(?1) || typeof(?2)",
                (Qty(1_250), Rate(1_900)),
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (q, r, kinds.as_str()),
            (Qty(1_250), Rate(1_900), "integerinteger")
        );
        let too_big = conn.query_row("SELECT 3000000000", [], |row| row.get::<_, Rate>(0));
        assert!(
            too_big.is_err(),
            "a rate beyond i32 is refused, not truncated"
        );
        let text = conn.query_row("SELECT 'x'", [], |row| row.get::<_, Qty>(0));
        assert!(text.is_err());
    }

    #[test]
    fn mul_div_rounds_once_half_away_from_zero() {
        // 119.00 DA TTC at 19 % -> 100.00 DA HT.
        assert_eq!(Money(11_900).mul_div(10_000, 11_900), Some(Money(10_000)));
        // 100.00 DA TTC at 19 % -> 84.0336 -> 84.03 DA HT.
        assert_eq!(Money(10_000).mul_div(10_000, 11_900), Some(Money(8_403)));
        assert_eq!(Money(-10_000).mul_div(10_000, 11_900), Some(Money(-8_403)));
        // 1.00 DA x 1/2 = 0.5 centime -> 1 centime, away from zero both ways.
        assert_eq!(Money(1).mul_div(1, 2), Some(Money(1)));
        assert_eq!(Money(-1).mul_div(1, 2), Some(Money(-1)));
        assert_eq!(Money(1).mul_div(1, 0), None);
        assert_eq!(Money(1).mul_div(1, -3), None);
        assert_eq!(Money(i64::MAX).mul_div(2, 1), None);
        assert!(Money(1).is_positive() && !Money(0).is_positive());
        assert_eq!(Money(-5).checked_abs(), Some(Money(5)));
        assert_eq!(Money(i64::MIN).checked_abs(), None);
    }

    #[test]
    fn qty_helpers() {
        assert_eq!(Qty(2_000).mul_qty(Qty(1_500)), Some(Qty(3_000))); // 2 per kit x 1.5 kits
        assert_eq!(Qty(333).mul_qty(Qty(1_500)), Some(Qty(500))); // 0.4995 -> 0.500
        assert_eq!(Qty(i64::MAX).mul_qty(Qty(2_000)), None);
        assert_eq!(Qty(1_500).checked_neg(), Some(Qty(-1_500)));
        assert!(Qty(1).is_positive() && Qty(-1).is_negative() && !Qty(0).is_positive());
        assert!(Qty(1_250).fits_decimals(3) && Qty(1_250).fits_decimals(2));
        assert!(!Qty(1_250).fits_decimals(1) && !Qty(1_500).fits_decimals(0));
        assert!(Qty(3_000).fits_decimals(0) && Qty(3_000).fits_decimals(9));
    }

    /// True when `r` is `exact / unit` rounded to the nearest integer, ties away from zero.
    fn rounds_half_away(r: Money, exact: i128, unit: i128) -> bool {
        let scaled = i128::from(r.0) * unit;
        let twice_error = 2 * (scaled - exact).abs();
        twice_error < unit || (twice_error == unit && scaled.abs() > exact.abs())
    }

    proptest! {
        #[test]
        fn sums_commute(a in -1_000_000_000_000i64..1_000_000_000_000, b in -1_000_000_000_000i64..1_000_000_000_000) {
            prop_assert_eq!(Money(a).checked_add(Money(b)), Money(b).checked_add(Money(a)));
        }

        #[test]
        fn sum_is_exact_and_order_independent(
            // Full-range values mixed with small ones, so long lists can still fit.
            v in prop::collection::vec(prop_oneof![any::<i64>(), -1_000_000_000i64..1_000_000_000], 0..50),
        ) {
            let exact: i128 = v.iter().copied().map(i128::from).sum();
            let forward = Money::checked_sum(v.iter().copied().map(Money));
            let backward = Money::checked_sum(v.iter().rev().copied().map(Money));
            prop_assert_eq!(forward, i64::try_from(exact).ok().map(Money));
            prop_assert_eq!(backward, forward);
        }

        #[test]
        fn mul_qty_never_panics(p in any::<i64>(), q in any::<i64>()) {
            // Over the whole i64 range: no panic, and any result that fits is rounded once.
            if let Some(r) = Money(p).mul_qty(Qty(q)) {
                prop_assert!(rounds_half_away(r, i128::from(p) * i128::from(q), 1_000), "{r:?}");
            }
        }

        #[test]
        fn mul_qty_is_within_half_a_centime(p in -10_000_000_000i64..10_000_000_000, q in -1_000_000_000i64..1_000_000_000) {
            let r = Money(p).mul_qty(Qty(q)).unwrap();
            prop_assert!(rounds_half_away(r, i128::from(p) * i128::from(q), 1_000), "{r:?}");
        }

        #[test]
        fn mul_qty_ties_round_away_from_zero(k in -1_000_000_000i64..1_000_000_000, m in -1_000_000i64..1_000_000) {
            // An odd price times a quantity ending in .500 lands exactly on half a centime.
            let (p, q) = (2 * k + 1, m * 1_000 + 500);
            let exact = i128::from(p) * i128::from(q);
            let r = Money(p).mul_qty(Qty(q)).unwrap();
            prop_assert_eq!((i128::from(r.0) * 1_000 - exact).abs(), 500);
            prop_assert!(rounds_half_away(r, exact, 1_000), "{r:?}");
        }

        #[test]
        fn mul_qty_is_sign_symmetric(p in -10_000_000_000i64..10_000_000_000, q in -1_000_000_000i64..1_000_000_000) {
            prop_assert_eq!(Money(-p).mul_qty(Qty(q)).map(|m| -m.0), Money(p).mul_qty(Qty(q)).map(|m| m.0));
        }

        #[test]
        fn mul_div_is_within_half_a_centime(
            p in -10_000_000_000i64..10_000_000_000,
            num in 0i64..=20_000,
            den in 1i64..=20_000,
        ) {
            let r = Money(p).mul_div(num, den).unwrap();
            prop_assert!(rounds_half_away(r, i128::from(p) * i128::from(num), i128::from(den)), "{r:?}");
        }

        #[test]
        fn apply_rate_is_within_half_a_centime(p in -10_000_000_000i64..10_000_000_000, bp in 0i32..=10_000) {
            let r = Money(p).apply_rate(Rate(bp)).unwrap();
            prop_assert!(rounds_half_away(r, i128::from(p) * i128::from(bp), 10_000), "{r:?}");
        }

        #[test]
        fn apply_rate_ties_round_away_from_zero(
            d in prop::sample::select(vec![1i32, 2, 4, 5, 8, 10, 20, 25, 40, 50, 100, 125, 200, 250, 500, 625, 1_000, 1_250, 2_500, 5_000]),
            k in -1_000_000_000i64..1_000_000_000,
        ) {
            // With bp = 5000 / d and p an odd multiple of d, p x bp is an odd number of half centimes.
            let (p, bp) = (i64::from(d) * (2 * k + 1), 5_000 / d);
            let exact = i128::from(p) * i128::from(bp);
            let r = Money(p).apply_rate(Rate(bp)).unwrap();
            prop_assert_eq!((i128::from(r.0) * 10_000 - exact).abs(), 5_000);
            prop_assert!(rounds_half_away(r, exact, 10_000), "{r:?}");
        }
    }
}
