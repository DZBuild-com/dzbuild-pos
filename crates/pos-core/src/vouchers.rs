//! The voucher tender's router (PAY-011, PAY-007 "avoir/bon d'achat", CRD-007): the `voucher`
//! tender of M1a keeps its one payment method; its reference says what it spends — a customer
//! order's deposit (`CC2026-…`), a service ticket's deposit (`RP2026-…`), a gift, exchange or
//! loyalty voucher (`BA2026-…` or its scanned `298…` barcode, Task 13), or, with no such
//! reference, the customer's store credit (M2).
//! `book` writes the spending of each register in the sale's transaction, never beyond its balance;
//! on a till, `check` checks it and the main PC books it once the ticket reached it (Task 31).

use crate::deposits::{self, OwnerType};
use crate::documents::Document;
use crate::error::AppError;
use crate::money::Money;
use crate::payments::{PaymentInput, PaymentMethod};
use crate::till::TillContext;
use rusqlite::{Connection, Transaction};
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VoucherTarget {
    StoreCredit,
    #[serde(rename_all = "camelCase")]
    Deposit {
        owner_type: OwnerType,
        number: String,
    },
    /// A gift, exchange or loyalty voucher (`gift_vouchers`): a bearer voucher, whoever the
    /// ticket's customer is.
    #[serde(rename_all = "camelCase")]
    GiftVoucher {
        code: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VoucherInfo {
    pub target: VoucherTarget,
    pub balance: Money,
    pub customer_id: Option<String>,
}

/// `PPyyyy-nnnnnn` with the prefix `PP`.
fn numbered(s: &str, prefix: &str) -> bool {
    let b = s.as_bytes();
    s.len() == 13
        && s.starts_with(prefix)
        && b[2..6].iter().all(u8::is_ascii_digit)
        && b[6] == b'-'
        && b[7..].iter().all(u8::is_ascii_digit)
}

pub fn classify(reference: Option<&str>) -> VoucherTarget {
    let Some(r) = reference.map(|r| r.trim().to_uppercase()) else {
        return VoucherTarget::StoreCredit;
    };
    if numbered(&r, "CC") {
        VoucherTarget::Deposit {
            owner_type: OwnerType::CustomerOrder,
            number: r,
        }
    } else if numbered(&r, "RP") {
        VoucherTarget::Deposit {
            owner_type: OwnerType::ServiceTicket,
            number: r,
        }
    } else if numbered(&r, "BA") {
        VoucherTarget::GiftVoucher { code: r }
    } else if let Some(code) = crate::gift_vouchers::code_of_barcode(&r) {
        VoucherTarget::GiftVoucher { code }
    } else {
        VoucherTarget::StoreCredit
    }
}

/// The part of the voucher tenders that spends the customer's store credit (M2).
pub(crate) fn store_credit_total(tenders: &[PaymentInput]) -> Result<Money, AppError> {
    Money::checked_sum(
        tenders
            .iter()
            .filter(|t| {
                t.method == PaymentMethod::Voucher
                    && classify(t.reference.as_deref()) == VoucherTarget::StoreCredit
            })
            .map(|t| t.amount),
    )
    .ok_or_else(|| AppError::validation("payments", "amount_invalid"))
}

/// What a voucher reference would spend, for the payment screen (before it is tendered).
pub fn describe(conn: &Connection, reference: &str) -> Result<Option<VoucherInfo>, AppError> {
    match classify(Some(reference)) {
        VoucherTarget::StoreCredit => Ok(None),
        VoucherTarget::Deposit { owner_type, number } => {
            let Some((owner_id, customer_id)) =
                deposits::owner_of_number(conn, owner_type, &number)?
            else {
                return Ok(None);
            };
            Ok(Some(VoucherInfo {
                balance: deposits::held(conn, owner_type, &owner_id)?,
                target: VoucherTarget::Deposit { owner_type, number },
                customer_id,
            }))
        }
        VoucherTarget::GiftVoucher { code } => match crate::gift_vouchers::get(conn, &code) {
            Ok(v) => Ok(Some(VoucherInfo {
                target: VoucherTarget::GiftVoucher { code: v.code },
                balance: v.balance,
                customer_id: v.customer_id,
            })),
            Err(AppError::Validation { .. }) => Ok(None),
            Err(e) => Err(e),
        },
    }
}

/// Spends the registers named by the ticket's voucher tenders (`sales::extensions::after_sale`,
/// and on the main PC for a till's ticket).
pub(crate) fn book(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    document: &Document,
    tenders: &[PaymentInput],
) -> Result<(), AppError> {
    route(tx, ctx, document, tenders, Some(tx), Vec::new()).map(drop)
}

/// A till's ticket (deposits and vouchers are the main PC's ledgers): each voucher
/// tender checked against the till's copy, as [`book`] checks it, and left to the main PC to
/// spend. The copy does not show what the till's `waiting` tickets spent while the main PC has
/// not booked them (Task 31 review): their tenders are counted too. Whether any spends a deposit
/// or a gift, exchange or loyalty voucher.
pub(crate) fn check(
    conn: &Connection,
    ctx: &TillContext,
    document: &Document,
    tenders: &[PaymentInput],
    waiting: &[Document],
) -> Result<bool, AppError> {
    let mut claimed: Vec<(String, Money)> = Vec::new();
    for p in waiting
        .iter()
        .flat_map(|t| &t.payments)
        .filter(|p| p.method == PaymentMethod::Voucher)
    {
        let key = match classify(p.reference.as_deref()) {
            VoucherTarget::StoreCredit => continue,
            VoucherTarget::Deposit { number, .. } => number,
            VoucherTarget::GiftVoucher { code } => code,
        };
        wanted(&mut claimed, &key, p.amount, "payments")?;
    }
    route(conn, ctx, document, tenders, None, claimed)
}

/// What the ticket's tenders of the register `key` want from it, `amount` included: a check
/// writes nothing, so the earlier tenders of the same register (and the claims of the till's
/// waiting tickets) are added up here.
fn wanted(
    earlier: &mut Vec<(String, Money)>,
    key: &str,
    amount: Money,
    field: &str,
) -> Result<Money, AppError> {
    let i = match earlier.iter().position(|(k, _)| k == key) {
        Some(i) => i,
        None => {
            earlier.push((key.to_owned(), Money::ZERO));
            earlier.len() - 1
        }
    };
    let total = earlier[i]
        .1
        .checked_add(amount)
        .ok_or_else(|| AppError::validation(field, "amount_invalid"))?;
    earlier[i].1 = total;
    Ok(total)
}

/// Checks every voucher tender of the ticket and, given the transaction, spends it (each spend is
/// then in the balance the next tender reads); without it, each tender wants its amount on top of
/// `earlier`. Whether any tender named a deposit or a voucher.
fn route(
    conn: &Connection,
    ctx: &TillContext,
    document: &Document,
    tenders: &[PaymentInput],
    write: Option<&Transaction<'_>>,
    mut earlier: Vec<(String, Money)>,
) -> Result<bool, AppError> {
    let mut any = false;
    for (i, t) in tenders
        .iter()
        .enumerate()
        .filter(|(_, t)| t.method == PaymentMethod::Voucher)
    {
        let field = format!("payments.{i}.reference");
        match classify(t.reference.as_deref()) {
            VoucherTarget::StoreCredit => {}
            VoucherTarget::Deposit { owner_type, number } => {
                let (owner_id, customer_id) = deposits::owner_of_number(conn, owner_type, &number)?
                    .ok_or_else(|| AppError::validation(&field, "voucher_unknown"))?;
                // A deposit with an owner is spent only on that customer's ticket: an anonymous
                // ticket naming the printed `RP…`/`CC…` number is refused too.
                if customer_id.is_some() && customer_id != document.customer_id {
                    return Err(AppError::validation(&field, "deposit_owner_invalid"));
                }
                let want = match write {
                    Some(_) => t.amount,
                    None => wanted(&mut earlier, &number, t.amount, &field)?,
                };
                if deposits::held(conn, owner_type, &owner_id)? < want {
                    return Err(AppError::rejected(
                        serde_json::json!({ "reference": number }),
                        "deposit_insufficient",
                    ));
                }
                if let Some(tx) = write {
                    deposits::apply(tx, ctx, owner_type, &owner_id, t.amount, &document.id)?;
                }
                any = true;
            }
            VoucherTarget::GiftVoucher { code } => {
                match write {
                    Some(tx) => {
                        crate::gift_vouchers::spend(tx, ctx, &code, t.amount, &document.id)
                            .map_err(|e| e.at_field(&field))?;
                    }
                    None => {
                        let want = wanted(&mut earlier, &code, t.amount, &field)?;
                        crate::gift_vouchers::check_spend(conn, ctx, &code, want)
                            .map_err(|e| e.at_field(&field))?;
                    }
                }
                any = true;
            }
        }
    }
    Ok(any)
}
