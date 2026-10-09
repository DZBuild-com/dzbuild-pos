//! Droit de timbre (Code du timbre art. 100, LF 2025 art. 46; DGI circulaire 14/2025): a duty on
//! each quittance paid in cash. Values come from the data pack (`stampDuty`):
//!
//! - amount ≤ `exemptUpTo` (300.00 DA) → 0;
//! - otherwise `ceil(amount / tranche)` tranches of 100 DA, each charged the `perTranche` of the
//!   bracket that contains the **whole** amount (1 DA up to 30 000 DA, 1.5 DA up to 100 000 DA,
//!   2 DA above), with a `minimum` of 5 DA and no ceiling;
//! - a future pack may declare the scale `progressive`: each bracket then charges only the
//!   slice of the amount inside it.
//!
//! Which tender is dutiable is law, pinned here (research 04 §4.4): cash is; card, Edahabia and
//! BaridiMob/QR ("paiement électronique sous toutes ses formes") are exempt; a cheque or a
//! transfer is exempt only when the receipt carries its details; a credit sale is not dutiable
//! at the sale but at each later cash settlement, which calls [`for_cash_amount`] on the cash it
//! receives (M2 `credit.collect`).

use crate::datapack::{Pack, StampDuty};
use crate::money::Money;
use crate::payments::PaymentMethod;
use crate::sqlenum::sql_text_enum;
use serde::{Deserialize, Serialize};
use specta::Type;

/// Setting `stamp_duty.mode` (spec §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StampMode {
    /// Added to the amount due ("Net à payer = TTC + timbre").
    #[default]
    ChargeCustomer,
    /// Paid by the shop: recorded on the document, not added to the amount due.
    Absorb,
    /// Not computed (exempt IFU shops; the settings screen warns).
    Off,
}

impl StampMode {
    pub const ALL: [StampMode; 3] = [StampMode::ChargeCustomer, StampMode::Absorb, StampMode::Off];

    pub const fn as_str(self) -> &'static str {
        match self {
            StampMode::ChargeCustomer => "charge_customer",
            StampMode::Absorb => "absorb",
            StampMode::Off => "off",
        }
    }

    pub fn parse(s: &str) -> Option<StampMode> {
        StampMode::ALL.into_iter().find(|m| m.as_str() == s)
    }
}

sql_text_enum!(StampMode);

/// How a tender is treated for the duty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StampTreatment {
    /// Cash: the duty is due on this amount.
    Dutiable,
    /// Electronic payment: never dutiable.
    Exempt,
    /// Cheque or transfer: exempt when the receipt carries its details, dutiable otherwise.
    ExemptWithDetails,
    /// Credit (kridi): not dutiable now; due on the later cash settlement.
    Deferred,
}

/// The legal treatment of `method` under `pack` (the pack lists the exempt tenders by name).
pub fn treatment(pack: &Pack, method: PaymentMethod) -> StampTreatment {
    let listed = pack
        .stamp_duty
        .exempt_tenders
        .iter()
        .any(|t| t == method.as_str());
    match method {
        PaymentMethod::Cash | PaymentMethod::Change => StampTreatment::Dutiable,
        PaymentMethod::Credit => StampTreatment::Deferred,
        PaymentMethod::Cheque | PaymentMethod::Transfer if listed => {
            StampTreatment::ExemptWithDetails
        }
        // Store credit settles a debt the shop owes; no money changes hands.
        PaymentMethod::Voucher => StampTreatment::Exempt,
        _ if listed => StampTreatment::Exempt,
        _ => StampTreatment::Dutiable,
    }
}

fn ceil_div(a: i128, b: i128) -> i128 {
    (a + b - 1) / b
}

fn clamp(value: i128) -> Money {
    Money(i64::try_from(value).unwrap_or(i64::MAX))
}

/// The duty for one cash quittance of `amount` under the scale `stamp`.
pub fn duty(stamp: &StampDuty, amount: Money) -> Money {
    if amount <= stamp.exempt_up_to || stamp.brackets.is_empty() || !stamp.tranche.is_positive() {
        return Money::ZERO;
    }
    let tranche = i128::from(stamp.tranche.0);
    let whole = i128::from(amount.0);
    let computed = if stamp.progressive {
        let mut total = 0_i128;
        let mut lower = 0_i128;
        for bracket in &stamp.brackets {
            let upper = bracket.up_to.map_or(whole, |u| i128::from(u.0).min(whole));
            if upper > lower {
                total += ceil_div(upper - lower, tranche) * i128::from(bracket.per_tranche.0);
            }
            if bracket.up_to.is_none_or(|u| whole <= i128::from(u.0)) {
                break;
            }
            lower = upper;
        }
        total
    } else {
        let bracket = stamp
            .brackets
            .iter()
            .find(|b| b.up_to.is_none_or(|u| amount <= u))
            .or(stamp.brackets.last());
        let per_tranche = bracket.map_or(0, |b| i128::from(b.per_tranche.0));
        ceil_div(whole, tranche) * per_tranche
    };
    clamp(computed.max(i128::from(stamp.minimum.0)))
}

/// The duty on a cash payment of `cash_centimes` (a sale's cash part, or a later credit
/// settlement received in cash). Amounts beyond `i64` saturate.
pub fn for_cash_amount(pack: &Pack, cash_centimes: Money) -> Money {
    duty(&pack.stamp_duty, cash_centimes)
}

/// The live duty for the payment screen (SAL-057): the duty, the bracket's rate per tranche,
/// and the upper bound of the bracket (the "cliff" above which each 100 DA costs more).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StampQuote {
    pub duty: Money,
    pub per_tranche: Money,
    pub bracket_up_to: Option<Money>,
}

pub fn quote(pack: &Pack, cash_centimes: Money) -> StampQuote {
    let stamp = &pack.stamp_duty;
    let bracket = stamp
        .brackets
        .iter()
        .find(|b| b.up_to.is_none_or(|u| cash_centimes <= u))
        .or(stamp.brackets.last());
    StampQuote {
        duty: duty(stamp, cash_centimes),
        per_tranche: if cash_centimes <= stamp.exempt_up_to {
            Money::ZERO
        } else {
            bracket.map_or(Money::ZERO, |b| b.per_tranche)
        },
        bracket_up_to: if cash_centimes <= stamp.exempt_up_to {
            Some(stamp.exempt_up_to)
        } else {
            bracket.and_then(|b| b.up_to)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datapack::StampBracket;
    use proptest::prelude::*;

    fn pack() -> Pack {
        crate::datapack::bundled().unwrap()
    }

    fn da(dinars: i64, centimes: i64) -> Money {
        Money(dinars * 100 + centimes)
    }

    #[test]
    fn boundaries_of_the_2026_scale() {
        let p = pack();
        let cases = [
            (da(0, 0), da(0, 0)),
            (da(300, 0), da(0, 0)),
            (da(300, 1), da(5, 0)), // 4 tranches × 1 DA = 4 DA → minimum 5 DA
            (da(450, 0), da(5, 0)), // 5 tranches
            (da(501, 0), da(6, 0)), // 6 tranches (fraction of 100 DA counts)
            (da(1_000, 0), da(10, 0)),
            (da(30_000, 0), da(300, 0)),  // 300 tranches × 1 DA
            (da(30_000, 1), da(451, 50)), // 301 tranches × 1.5 DA: the whole amount changes bracket
            (da(100_000, 0), da(1_500, 0)),
            (da(100_000, 1), da(2_002, 0)), // 1001 tranches × 2 DA
            (da(250_000, 0), da(5_000, 0)),
        ];
        for (amount, expected) in cases {
            assert_eq!(for_cash_amount(&p, amount), expected, "cash {amount}");
        }
    }

    #[test]
    fn tenders_are_mapped_to_their_legal_treatment() {
        let p = pack();
        assert_eq!(treatment(&p, PaymentMethod::Cash), StampTreatment::Dutiable);
        for m in [
            PaymentMethod::CardCib,
            PaymentMethod::Edahabia,
            PaymentMethod::Baridimob,
        ] {
            assert_eq!(treatment(&p, m), StampTreatment::Exempt, "{m:?}");
        }
        assert_eq!(
            treatment(&p, PaymentMethod::Cheque),
            StampTreatment::ExemptWithDetails
        );
        assert_eq!(
            treatment(&p, PaymentMethod::Transfer),
            StampTreatment::ExemptWithDetails
        );
        assert_eq!(
            treatment(&p, PaymentMethod::Credit),
            StampTreatment::Deferred
        );
        // Every tender the pack names is a known payment method.
        for name in &p.stamp_duty.exempt_tenders {
            assert!(
                PaymentMethod::parse(name).is_some(),
                "unknown tender {name} in the data pack"
            );
        }
        // A pack that stops exempting cards makes them dutiable.
        let mut strict = pack();
        strict.stamp_duty.exempt_tenders.retain(|t| t != "card_cib");
        assert_eq!(
            treatment(&strict, PaymentMethod::CardCib),
            StampTreatment::Dutiable
        );
    }

    #[test]
    fn a_progressive_scale_charges_each_slice() {
        let mut stamp = pack().stamp_duty;
        stamp.progressive = true;
        // 30 000.01 DA: 300 tranches at 1 DA + 1 tranche (0.01 DA) at 1.5 DA = 301.50 DA.
        assert_eq!(duty(&stamp, da(30_000, 1)), da(301, 50));
        // 150 000 DA: 300 × 1 + 700 × 1.5 + 500 × 2 = 2 350 DA.
        assert_eq!(duty(&stamp, da(150_000, 0)), da(2_350, 0));
        assert_eq!(
            duty(&stamp, da(300, 1)),
            da(5, 0),
            "the minimum still applies"
        );
    }

    #[test]
    fn quote_names_the_bracket_cliff() {
        let p = pack();
        let q = quote(&p, da(29_950, 0));
        assert_eq!(
            (q.duty, q.per_tranche, q.bracket_up_to),
            (da(300, 0), Money(100), Some(da(30_000, 0)))
        );
        let q = quote(&p, da(200, 0));
        assert_eq!(
            (q.duty, q.per_tranche, q.bracket_up_to),
            (Money::ZERO, Money::ZERO, Some(da(300, 0)))
        );
        let q = quote(&p, da(200_000, 0));
        assert_eq!((q.per_tranche, q.bracket_up_to), (Money(200), None));
    }

    #[test]
    fn degenerate_scales_do_not_panic() {
        let mut stamp = pack().stamp_duty;
        assert_eq!(
            duty(&stamp, Money(i64::MAX)),
            Money(184_467_440_737_095_600),
            "no overflow"
        );
        stamp.brackets[2].per_tranche = Money(i64::MAX);
        assert_eq!(duty(&stamp, Money(i64::MAX)), Money(i64::MAX), "saturates");
        stamp.brackets = vec![StampBracket {
            up_to: None,
            per_tranche: Money(100),
        }];
        stamp.tranche = Money::ZERO;
        assert_eq!(duty(&stamp, da(1_000, 0)), Money::ZERO);
        assert_eq!(StampMode::parse("absorb"), Some(StampMode::Absorb));
        assert_eq!(
            serde_json::to_string(&StampMode::ChargeCustomer).unwrap(),
            "\"charge_customer\""
        );
    }

    proptest! {
        #[test]
        fn at_least_five_dinars_above_three_hundred(centimes in 30_001i64..10_000_000_000) {
            prop_assert!(for_cash_amount(&pack(), Money(centimes)) >= Money(500));
        }

        #[test]
        fn nothing_up_to_three_hundred(centimes in 0i64..=30_000) {
            prop_assert_eq!(for_cash_amount(&pack(), Money(centimes)), Money::ZERO);
        }

        #[test]
        fn never_decreases_inside_a_bracket(a in 30_001i64..10_000_000, step in 0i64..100_000) {
            let p = pack();
            let b = a + step;
            let same_bracket = (a <= 3_000_000) == (b <= 3_000_000) && (a <= 10_000_000) == (b <= 10_000_000);
            if same_bracket {
                prop_assert!(for_cash_amount(&p, Money(a)) <= for_cash_amount(&p, Money(b)));
            }
        }
    }
}
