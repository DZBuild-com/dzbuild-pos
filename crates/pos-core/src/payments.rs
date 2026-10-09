//! Tenders and settlement of an amount (spec §6, §7.1; PAY-001/004/007/008/016/017/023): which
//! part is cash, the stamp duty on it, cash rounding, the amount due, what is missing and the
//! change. Pure: the sale and the payment screen call the same function.

use crate::datapack::Pack;
use crate::error::AppError;
use crate::money::Money;
use crate::sqlenum::sql_text_enum;
use crate::tax::timbre::{self, StampMode, StampTreatment};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    Cash,
    CardCib,
    Edahabia,
    /// BaridiMob / QR payment.
    Baridimob,
    Cheque,
    /// Bank or CCP transfer.
    Transfer,
    /// Customer credit (kridi): M2.
    Credit,
    /// Store credit / avoir: M2.
    Voucher,
    /// Change given back (negative), never a tender.
    Change,
}

impl PaymentMethod {
    pub const ALL: [PaymentMethod; 9] = [
        PaymentMethod::Cash,
        PaymentMethod::CardCib,
        PaymentMethod::Edahabia,
        PaymentMethod::Baridimob,
        PaymentMethod::Cheque,
        PaymentMethod::Transfer,
        PaymentMethod::Credit,
        PaymentMethod::Voucher,
        PaymentMethod::Change,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            PaymentMethod::Cash => "cash",
            PaymentMethod::CardCib => "card_cib",
            PaymentMethod::Edahabia => "edahabia",
            PaymentMethod::Baridimob => "baridimob",
            PaymentMethod::Cheque => "cheque",
            PaymentMethod::Transfer => "transfer",
            PaymentMethod::Credit => "credit",
            PaymentMethod::Voucher => "voucher",
            PaymentMethod::Change => "change",
        }
    }

    pub fn parse(s: &str) -> Option<PaymentMethod> {
        PaymentMethod::ALL.into_iter().find(|m| m.as_str() == s)
    }

    pub const fn is_cash(self) -> bool {
        matches!(self, PaymentMethod::Cash)
    }
}

sql_text_enum!(PaymentMethod);

/// One tender as typed on the payment screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PaymentInput {
    pub method: PaymentMethod,
    pub amount: Money,
    /// Cheque number, transfer id, card authorisation number, QR transaction id (PAY-008).
    pub reference: Option<String>,
    pub cheque_bank: Option<String>,
    /// `YYYY-MM-DD`.
    pub cheque_date: Option<String>,
}

impl PaymentInput {
    pub fn cash(amount: Money) -> PaymentInput {
        PaymentInput {
            method: PaymentMethod::Cash,
            amount,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
        }
    }

    pub fn electronic(
        method: PaymentMethod,
        amount: Money,
        reference: Option<&str>,
    ) -> PaymentInput {
        PaymentInput {
            method,
            amount,
            reference: reference.map(str::to_owned),
            cheque_bank: None,
            cheque_date: None,
        }
    }

    /// True when the receipt can carry the cheque or transfer details the exemption needs.
    pub fn has_details(&self) -> bool {
        let set = |v: &Option<String>| v.as_deref().is_some_and(|s| !s.trim().is_empty());
        match self.method {
            PaymentMethod::Cheque => {
                set(&self.reference) && set(&self.cheque_bank) && set(&self.cheque_date)
            }
            PaymentMethod::Transfer => set(&self.reference),
            _ => true,
        }
    }
}

/// Rules read from the settings when settling (`stamp_duty.mode`, `sales.cash_rounding`), and
/// the store credit applied before the tenders (the value of goods returned in an exchange).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SettleRules {
    pub stamp_mode: StampMode,
    /// 0 (off) or the step the cash total is rounded to (500 = 5 DA), PAY-016.
    pub cash_rounding: Money,
    /// Settled without money (exempt from the duty, never gives change).
    pub store_credit: Money,
}

/// The settlement of a total by tenders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Settlement {
    pub total_ttc: Money,
    /// The part of the TTC paid in cash (or by a cheque/transfer without details): the duty base.
    pub stamp_base: Money,
    pub stamp_duty: Money,
    pub stamp_mode: StampMode,
    /// The duty added to the amount due (0 when absorbed or off).
    pub stamp_charged: Money,
    pub rounding: Money,
    pub total_due: Money,
    /// Store credit used (exchange): part of `non_cash`.
    pub store_credit: Money,
    pub non_cash: Money,
    pub cash_due: Money,
    pub cash_tendered: Money,
    pub paid_total: Money,
    /// What is still to pay (0 once the tenders cover the amount due).
    pub missing: Money,
    pub change: Money,
    /// The tenders, trimmed and checked.
    pub tenders: Vec<PaymentInput>,
}

fn clean_text(value: Option<&str>, field: &str) -> Result<Option<String>, AppError> {
    let value = value.map(str::trim).filter(|v| !v.is_empty());
    if value.is_some_and(|v| v.chars().count() > 48 || v.chars().any(char::is_control)) {
        return Err(AppError::validation(field, "value_invalid"));
    }
    Ok(value.map(str::to_owned))
}

/// Checks one tender (`payments.{i}.*` fields). Credit and vouchers are checked by the sale
/// against the customer's account (M2). A cheque needs its number, bank and date, a transfer its
/// reference, so that the receipt carries them (timbre exemption).
pub fn clean_tender(i: usize, p: &PaymentInput) -> Result<PaymentInput, AppError> {
    let field = |name: &str| format!("payments.{i}.{name}");
    if p.method == PaymentMethod::Change {
        return Err(AppError::validation(
            &field("method"),
            "payment_method_invalid",
        ));
    }
    if !p.amount.is_positive() {
        return Err(AppError::validation(&field("amount"), "amount_invalid"));
    }
    let reference = clean_text(p.reference.as_deref(), &field("reference"))?;
    let cheque_bank = clean_text(p.cheque_bank.as_deref(), &field("chequeBank"))?;
    let cheque_date = clean_text(p.cheque_date.as_deref(), &field("chequeDate"))?;
    match p.method {
        PaymentMethod::Cheque => {
            for (name, value) in [
                ("reference", &reference),
                ("chequeBank", &cheque_bank),
                ("chequeDate", &cheque_date),
            ] {
                if value.is_none() {
                    return Err(AppError::validation(
                        &field(name),
                        "cheque_details_required",
                    ));
                }
            }
            if cheque_date
                .as_deref()
                .is_some_and(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").is_err())
            {
                return Err(AppError::validation(&field("chequeDate"), "value_invalid"));
            }
        }
        PaymentMethod::Transfer if reference.is_none() => {
            return Err(AppError::validation(
                &field("reference"),
                "transfer_reference_required",
            ));
        }
        _ => {}
    }
    Ok(PaymentInput {
        method: p.method,
        amount: p.amount,
        reference,
        cheque_bank,
        cheque_date,
    })
}

fn overflow() -> AppError {
    AppError::validation("payments", "amount_invalid")
}

/// Settles `total_ttc` with `payments` without requiring them to cover it: the live payment
/// screen shows `missing`, `stamp_duty` and `change` while tenders are typed. With no tender the
/// whole amount is assumed to be cash (the usual case).
pub fn preview(
    pack: &Pack,
    total_ttc: Money,
    payments: &[PaymentInput],
    rules: SettleRules,
) -> Result<Settlement, AppError> {
    let tenders = payments
        .iter()
        .enumerate()
        .map(|(i, p)| clean_tender(i, p))
        .collect::<Result<Vec<_>, _>>()?;
    let sum = |keep: &dyn Fn(&PaymentInput) -> bool| {
        Money::checked_sum(tenders.iter().filter(|t| keep(t)).map(|t| t.amount))
            .ok_or_else(overflow)
    };
    if rules.store_credit.is_negative() {
        return Err(overflow());
    }
    let non_cash = sum(&|t| !t.method.is_cash())?
        .checked_add(rules.store_credit)
        .ok_or_else(overflow)?;
    // No change on card tenders (PAY-017): non-cash tenders may not exceed the goods' total.
    if non_cash > total_ttc {
        return Err(AppError::validation("payments", "non_cash_exceeds_total"));
    }
    let not_dutiable = sum(&|t| match timbre::treatment(pack, t.method) {
        StampTreatment::Exempt | StampTreatment::Deferred => true,
        StampTreatment::ExemptWithDetails => t.has_details(),
        StampTreatment::Dutiable => false,
    })?
    .checked_add(rules.store_credit)
    .ok_or_else(overflow)?;
    let stamp_base = total_ttc
        .checked_sub(not_dutiable)
        .ok_or_else(overflow)?
        .max(Money::ZERO);
    let stamp_duty = match rules.stamp_mode {
        StampMode::Off => Money::ZERO,
        _ => timbre::for_cash_amount(pack, stamp_base),
    };
    let stamp_charged = if rules.stamp_mode == StampMode::ChargeCustomer {
        stamp_duty
    } else {
        Money::ZERO
    };
    let due = total_ttc.checked_add(stamp_charged).ok_or_else(overflow)?;
    let all_cash = rules.store_credit == Money::ZERO && tenders.iter().all(|t| t.method.is_cash());
    let rounding = if all_cash && rules.cash_rounding.is_positive() {
        let step = rules.cash_rounding.0;
        let rounded = due
            .mul_div(1, step)
            .and_then(|q| q.0.checked_mul(step))
            .ok_or_else(overflow)?;
        Money(rounded - due.0)
    } else {
        Money::ZERO
    };
    let total_due = due.checked_add(rounding).ok_or_else(overflow)?;
    let cash_due = total_due.checked_sub(non_cash).ok_or_else(overflow)?;
    let cash_tendered = sum(&|t| t.method.is_cash())?;
    let paid_total = non_cash.checked_add(cash_tendered).ok_or_else(overflow)?;
    let balance = cash_tendered.checked_sub(cash_due).ok_or_else(overflow)?;
    let missing = if balance.is_negative() {
        balance.checked_neg().ok_or_else(overflow)?
    } else {
        Money::ZERO
    };
    Ok(Settlement {
        total_ttc,
        stamp_base,
        stamp_duty,
        stamp_mode: rules.stamp_mode,
        stamp_charged,
        rounding,
        total_due,
        store_credit: rules.store_credit,
        non_cash,
        cash_due,
        cash_tendered,
        paid_total,
        missing,
        change: balance.max(Money::ZERO),
        tenders,
    })
}

/// [`preview`], then the tenders must cover the amount due (`payment_insufficient`).
pub fn settle(
    pack: &Pack,
    total_ttc: Money,
    payments: &[PaymentInput],
    rules: SettleRules,
) -> Result<Settlement, AppError> {
    let settlement = preview(pack, total_ttc, payments, rules)?;
    if settlement.missing.is_positive() {
        return Err(AppError::validation("payments", "payment_insufficient"));
    }
    Ok(settlement)
}
#[cfg(test)]
mod tests {
    use super::*;

    fn pack() -> Pack {
        crate::datapack::bundled().unwrap()
    }

    fn charge() -> SettleRules {
        SettleRules::default()
    }

    fn cheque(amount: i64) -> PaymentInput {
        PaymentInput {
            method: PaymentMethod::Cheque,
            amount: Money(amount),
            reference: Some("0012345".into()),
            cheque_bank: Some("BNA".into()),
            cheque_date: Some("2026-09-28".into()),
        }
    }

    #[test]
    fn cash_sale_pays_the_duty_and_gets_change() {
        // 450.00 DA paid with 1 000 DA: duty 5 DA, due 455 DA, change 545 DA.
        let s = settle(
            &pack(),
            Money(45_000),
            &[PaymentInput::cash(Money(100_000))],
            charge(),
        )
        .unwrap();
        assert_eq!(
            (s.stamp_base, s.stamp_duty, s.total_due),
            (Money(45_000), Money(500), Money(45_500))
        );
        assert_eq!(
            (s.cash_due, s.change, s.missing, s.paid_total),
            (Money(45_500), Money(54_500), Money::ZERO, Money(100_000))
        );
    }

    #[test]
    fn card_part_is_exempt_and_never_gets_change() {
        // 1 000 DA: 700 by card, 300 cash → the cash part is 300 DA, exempt.
        let s = settle(
            &pack(),
            Money(100_000),
            &[
                PaymentInput::electronic(PaymentMethod::CardCib, Money(70_000), Some("A1")),
                PaymentInput::cash(Money(30_000)),
            ],
            charge(),
        )
        .unwrap();
        assert_eq!(
            (s.stamp_base, s.stamp_duty, s.total_due, s.change),
            (Money(30_000), Money::ZERO, Money(100_000), Money::ZERO)
        );
        let over = preview(
            &pack(),
            Money(100_000),
            &[PaymentInput::electronic(
                PaymentMethod::Edahabia,
                Money(100_001),
                None,
            )],
            charge(),
        )
        .unwrap_err();
        assert_eq!(over.code(), "non_cash_exceeds_total");
        let all_card = settle(
            &pack(),
            Money(100_000),
            &[PaymentInput::electronic(
                PaymentMethod::Baridimob,
                Money(100_000),
                None,
            )],
            charge(),
        )
        .unwrap();
        assert_eq!(
            (all_card.stamp_duty, all_card.total_due),
            (Money::ZERO, Money(100_000))
        );
    }

    #[test]
    fn cheque_needs_its_details_and_is_then_exempt() {
        let s = settle(&pack(), Money(5_000_000), &[cheque(5_000_000)], charge()).unwrap();
        assert_eq!((s.stamp_duty, s.total_due), (Money::ZERO, Money(5_000_000)));
        let mut bare = cheque(5_000_000);
        bare.cheque_bank = None;
        let err = settle(&pack(), Money(5_000_000), &[bare.clone()], charge()).unwrap_err();
        assert_eq!(
            (err.code(), err.payload().field),
            (
                "cheque_details_required".into(),
                Some("payments.0.chequeBank".into())
            )
        );
        // The pure rule behind it: a cheque without details is dutiable like cash.
        assert!(!bare.has_details());
        let transfer = PaymentInput::electronic(PaymentMethod::Transfer, Money(10), None);
        let err = settle(&pack(), Money(10), &[transfer], charge()).unwrap_err();
        assert_eq!(err.code(), "transfer_reference_required");
        let mut late = cheque(100);
        late.cheque_date = Some("28/09/2026".into());
        assert_eq!(
            settle(&pack(), Money(100), &[late], charge())
                .unwrap_err()
                .code(),
            "value_invalid"
        );
    }

    #[test]
    fn absorbed_or_off_duty_is_not_added() {
        let absorb = SettleRules {
            stamp_mode: StampMode::Absorb,
            ..charge()
        };
        let s = settle(
            &pack(),
            Money(45_000),
            &[PaymentInput::cash(Money(45_000))],
            absorb,
        )
        .unwrap();
        assert_eq!(
            (s.stamp_duty, s.stamp_charged, s.total_due),
            (Money(500), Money::ZERO, Money(45_000))
        );
        let off = SettleRules {
            stamp_mode: StampMode::Off,
            ..charge()
        };
        let s = settle(
            &pack(),
            Money(45_000),
            &[PaymentInput::cash(Money(45_000))],
            off,
        )
        .unwrap();
        assert_eq!((s.stamp_duty, s.total_due), (Money::ZERO, Money(45_000)));
    }

    #[test]
    fn missing_amounts_and_insufficient_payment() {
        let s = preview(&pack(), Money(45_000), &[], charge()).unwrap();
        assert_eq!(
            (s.total_due, s.missing),
            (Money(45_500), Money(45_500)),
            "no tender: assumed cash"
        );
        let err = settle(
            &pack(),
            Money(45_000),
            &[PaymentInput::cash(Money(45_000))],
            charge(),
        )
        .unwrap_err();
        assert_eq!(
            err.code(),
            "payment_insufficient",
            "the duty is part of the amount due"
        );
    }

    #[test]
    fn cash_rounding_applies_to_all_cash_payments_only() {
        let five = SettleRules {
            cash_rounding: Money(500),
            ..charge()
        };
        // 452.30 + 5.00 duty = 457.30 → 455.00 (nearest 5 DA), rounding −2.30.
        let s = settle(
            &pack(),
            Money(45_230),
            &[PaymentInput::cash(Money(50_000))],
            five,
        )
        .unwrap();
        assert_eq!(
            (s.rounding, s.total_due, s.change),
            (Money(-230), Money(45_500), Money(4_500))
        );
        // 457.50 → 460.00 (half away from zero).
        let s = preview(&pack(), Money(45_250), &[], five).unwrap();
        assert_eq!((s.rounding, s.total_due), (Money(250), Money(46_000)));
        let mixed = settle(
            &pack(),
            Money(45_230),
            &[
                PaymentInput::electronic(PaymentMethod::CardCib, Money(10_000), None),
                PaymentInput::cash(Money(40_000)),
            ],
            five,
        )
        .unwrap();
        assert_eq!(mixed.rounding, Money::ZERO);
    }

    #[test]
    fn store_credit_from_an_exchange_is_exempt_and_gives_no_change() {
        // 2 000 DA of new goods, 1 500 DA of returned goods: 500 DA left, paid cash, exempt.
        let credit = SettleRules {
            store_credit: Money(150_000),
            ..charge()
        };
        let s = preview(
            &pack(),
            Money(200_000),
            &[PaymentInput::cash(Money(50_000))],
            credit,
        )
        .unwrap();
        assert_eq!(
            (s.stamp_base, s.stamp_duty, s.total_due),
            (Money(50_000), Money(500), Money(200_500))
        );
        assert_eq!((s.cash_due, s.missing), (Money(50_500), Money(500)));
        let s = preview(
            &pack(),
            Money(200_000),
            &[],
            SettleRules {
                store_credit: Money(200_000),
                ..charge()
            },
        )
        .unwrap();
        assert_eq!(
            (s.stamp_duty, s.total_due, s.missing, s.paid_total),
            (Money::ZERO, Money(200_000), Money::ZERO, Money(200_000))
        );
        let err = preview(
            &pack(),
            Money(100),
            &[],
            SettleRules {
                store_credit: Money(101),
                ..charge()
            },
        )
        .unwrap_err();
        assert_eq!(err.code(), "non_cash_exceeds_total");
    }

    #[test]
    fn account_tenders_are_non_cash_and_change_is_not_a_tender() {
        for method in [PaymentMethod::Credit, PaymentMethod::Voucher] {
            let p = PaymentInput {
                method,
                ..PaymentInput::cash(Money(100))
            };
            let s = preview(&pack(), Money(100), &[p], charge()).unwrap();
            assert_eq!(
                (s.stamp_duty, s.change, s.missing, s.non_cash),
                (Money::ZERO, Money::ZERO, Money::ZERO, Money(100))
            );
        }
        let change = PaymentInput {
            method: PaymentMethod::Change,
            ..PaymentInput::cash(Money(100))
        };
        assert_eq!(
            preview(&pack(), Money(100), &[change], charge())
                .unwrap_err()
                .code(),
            "payment_method_invalid"
        );
        let zero = PaymentInput::cash(Money::ZERO);
        let err = preview(&pack(), Money(100), &[zero], charge()).unwrap_err();
        assert_eq!(err.payload().field.as_deref(), Some("payments.0.amount"));
        for m in PaymentMethod::ALL {
            assert_eq!(PaymentMethod::parse(m.as_str()), Some(m));
        }
    }
}
