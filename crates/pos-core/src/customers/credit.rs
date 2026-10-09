//! The kridi account of a customer (CRD-001…008): opening balance from the paper notebook,
//! adjustments, versements with the droit de timbre on the cash received, statements and
//! ageing. Every operation appends one `customer_ledger` entry and one audit row.

use crate::audit::{self, Actor};
use crate::auth::Permission;
use crate::cash::{self, MovementKind};
use crate::error::AppError;
use crate::ledger::{
    self, AgingRow, Allocation, EntryKind, EntryPage, EntryQuery, LedgerEntry, LedgerRebuild,
    NewEntry, Side, Statement,
};
use crate::money::Money;
use crate::payments::{self, PaymentInput, PaymentMethod};
use crate::tax::timbre::{self, StampMode, StampTreatment};
use crate::till::TillContext;
use rusqlite::{Connection, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

fn clean_note(note: Option<&str>, required: bool) -> Result<Option<String>, AppError> {
    let note = crate::catalog::clean_opt(note, "note", 200)?;
    if required && note.is_none() {
        return Err(AppError::validation("note", "required"));
    }
    Ok(note)
}

fn active_customer(conn: &Connection, id: &str) -> Result<super::Customer, AppError> {
    let customer = super::get(conn, id)?;
    if customer.archived {
        return Err(AppError::not_found("customer"));
    }
    Ok(customer)
}

/// The debt (positive) or store credit (negative) carried over from the paper notebook
/// (CRD-003), once per customer.
pub fn set_opening(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    customer_id: &str,
    amount: Money,
    note: Option<&str>,
) -> Result<LedgerEntry, AppError> {
    active_customer(tx, customer_id)?;
    if amount == Money::ZERO {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    if ledger::has_opening(tx, Side::Customer, customer_id)? {
        return Err(AppError::conflict("opening_already_set"));
    }
    let mut entry = NewEntry::new(customer_id, EntryKind::Opening, amount);
    entry.note = clean_note(note, false)?;
    let actor = ctx.actor();
    let posted = ledger::append(
        tx,
        Side::Customer,
        &actor,
        &entry,
        &ctx.business_day(),
        ctx.now,
    )?;
    audit::append(
        tx,
        &actor.event(
            "credit.opening_set",
            "customer",
            Some(customer_id),
            json!({ "entryId": posted.id, "amount": amount, "balanceAfter": posted.balance_after }),
        ),
    )?;
    Ok(posted)
}

/// A signed correction with its reason (a bad debt written off, a mistake in the notebook).
pub fn adjust(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    customer_id: &str,
    amount: Money,
    note: &str,
) -> Result<LedgerEntry, AppError> {
    active_customer(tx, customer_id)?;
    if amount == Money::ZERO {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    let mut entry = NewEntry::new(customer_id, EntryKind::Adjustment, amount);
    entry.note = clean_note(Some(note), true)?;
    let actor = ctx.actor();
    let posted = ledger::append(
        tx,
        Side::Customer,
        &actor,
        &entry,
        &ctx.business_day(),
        ctx.now,
    )?;
    audit::append(
        tx,
        &actor.event(
            "credit.adjusted",
            "customer",
            Some(customer_id),
            json!({ "entryId": posted.id, "amount": amount, "note": entry.note,
                    "balanceAfter": posted.balance_after, "approvals": ctx.approvers() }),
        ),
    )?;
    Ok(posted)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CollectInput {
    pub customer_id: String,
    /// The debt paid off, before any stamp duty.
    pub amount: Money,
    pub method: PaymentMethod,
    /// Card authorisation, BaridiMob transaction, cheque number or transfer reference.
    pub reference: Option<String>,
    pub cheque_bank: Option<String>,
    pub cheque_date: Option<String>,
    /// Debts (ledger entries) to settle first, in this order; empty = the oldest first.
    pub allocate_to: Vec<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CollectQuote {
    pub amount: Money,
    pub stamp_duty: Money,
    /// The duty the customer hands over (0 when absorbed or off).
    pub stamp_charged: Money,
    pub stamp_mode: StampMode,
    /// What the customer hands over: amount + charged duty.
    pub to_receive: Money,
    pub balance_before: Money,
    pub balance_after: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Collected {
    pub entry: LedgerEntry,
    pub quote: CollectQuote,
    pub customer: super::Customer,
    /// The debts this versement settled, with their document numbers (for the receipt).
    pub allocations: Vec<Allocation>,
}

impl Collected {
    /// What this versement put in the drawer that took it (a host service's answer: the till
    /// records it, [`cash::record_share`]).
    pub fn drawer_share(&self) -> Option<cash::DrawerShare> {
        versement_share(&self.entry, &self.quote)
    }
}

/// The cash a versement brings into the drawer: what the customer hands over (the amount and
/// the duty charged); `None` for any other tender.
fn versement_share(entry: &LedgerEntry, quote: &CollectQuote) -> Option<cash::DrawerShare> {
    (entry.method == Some(PaymentMethod::Cash)).then(|| cash::DrawerShare {
        kind: MovementKind::CreditPayment,
        amount: quote.to_receive,
        ref_type: "customer_ledger".into(),
        ref_id: entry.id.clone(),
        note: None,
        category: None,
    })
}

/// Field errors of `payments::clean_tender` come back as `payments.0.<field>`: the versement
/// form has no list, so they are re-homed under `<field>`.
fn rehome(err: AppError) -> AppError {
    match err {
        AppError::Validation { field, code } => {
            AppError::validation(field.strip_prefix("payments.0.").unwrap_or(&field), &code)
        }
        other => other,
    }
}

pub(crate) fn clean_versement(input: &CollectInput) -> Result<PaymentInput, AppError> {
    if !matches!(
        input.method,
        PaymentMethod::Cash
            | PaymentMethod::CardCib
            | PaymentMethod::Edahabia
            | PaymentMethod::Baridimob
            | PaymentMethod::Cheque
            | PaymentMethod::Transfer
    ) {
        return Err(AppError::validation("method", "payment_method_invalid"));
    }
    if !input.amount.is_positive() {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    payments::clean_tender(
        0,
        &PaymentInput {
            method: input.method,
            amount: input.amount,
            reference: input.reference.clone(),
            cheque_bank: input.cheque_bank.clone(),
            cheque_date: input.cheque_date.clone(),
        },
    )
    .map_err(rehome)
}

/// Droit de timbre of a versement (TAX-007): on the cash received only, through the one entry
/// point `timbre::for_cash_amount`, as the shop's stamp duty mode says. Returns the duty, the
/// part charged to the customer and the mode.
pub(crate) fn versement_duty(
    conn: &Connection,
    ctx: &TillContext,
    tender: &PaymentInput,
) -> Result<(Money, Money, StampMode), AppError> {
    let mode: StampMode = crate::settings::get(conn, "stamp_duty.mode")?;
    let dutiable = match timbre::treatment(&ctx.pack, tender.method) {
        StampTreatment::Dutiable => true,
        StampTreatment::ExemptWithDetails => !tender.has_details(),
        StampTreatment::Exempt | StampTreatment::Deferred => false,
    };
    let duty = if dutiable && mode != StampMode::Off {
        timbre::for_cash_amount(&ctx.pack, tender.amount)
    } else {
        Money::ZERO
    };
    let charged = if mode == StampMode::ChargeCustomer {
        duty
    } else {
        Money::ZERO
    };
    Ok((duty, charged, mode))
}

/// The checked tender and the figures of a versement, shared by [`quote`] and [`collect`].
fn price(
    conn: &Connection,
    ctx: &TillContext,
    input: &CollectInput,
) -> Result<(PaymentInput, CollectQuote), AppError> {
    let customer = active_customer(conn, &input.customer_id)?;
    let tender = clean_versement(input)?;
    let (stamp_duty, stamp_charged, stamp_mode) = versement_duty(conn, ctx, &tender)?;
    let overflow = || AppError::validation("amount", "amount_invalid");
    let quote = CollectQuote {
        amount: tender.amount,
        stamp_duty,
        stamp_charged,
        stamp_mode,
        to_receive: tender
            .amount
            .checked_add(stamp_charged)
            .ok_or_else(overflow)?,
        balance_before: customer.balance,
        balance_after: customer
            .balance
            .checked_sub(tender.amount)
            .ok_or_else(overflow)?,
    };
    Ok((tender, quote))
}

/// The figures the versement dialog shows while the cashier types (SAL-057 for kridi).
pub fn quote(
    conn: &Connection,
    ctx: &TillContext,
    input: &CollectInput,
) -> Result<CollectQuote, AppError> {
    price(conn, ctx, input).map(|(_, quote)| quote)
}

/// A versement (CRD-004, PAY-022): numbered receipt `RV…`, duty on the cash part, drawer moved
/// by what the customer hands over, debts settled (chosen first, then the oldest), one audit row.
/// A versement above the debt leaves an advance (CRD-007).
pub fn collect(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: CollectInput,
) -> Result<Collected, AppError> {
    let (tender, q) = price(tx, ctx, &input)?;
    // This till's drawer, or the calling till's for a host service (Task 20 review).
    let session = if tender.method.is_cash() {
        Some(cash::drawer(tx, ctx)?)
    } else {
        cash::drawer_if_open(tx, ctx)?
    };
    let series = crate::sync::role::receipt_series(tx, &ctx.session.terminal_code)?;
    let receipt = crate::numbering::assign_custom_in(
        tx,
        "credit_receipt",
        "RV",
        &series,
        ctx.business_date(),
    )?;
    let mut entry = NewEntry::new(
        &input.customer_id,
        EntryKind::Payment,
        tender
            .amount
            .checked_neg()
            .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?,
    );
    entry.method = Some(tender.method);
    entry.reference = tender.reference.clone();
    entry.cheque_bank = tender.cheque_bank.clone();
    entry.cheque_date = tender.cheque_date.clone();
    entry.stamp_duty = q.stamp_duty;
    entry.stamp_charged = q.stamp_charged;
    entry.receipt_number = Some(receipt.number.clone());
    entry.cash_session_id = session.as_ref().map(|s| s.id.clone());
    entry.note = clean_note(input.note.as_deref(), false)?;
    entry.allocate_to = input.allocate_to;
    let actor = ctx.actor();
    let posted = ledger::append(
        tx,
        Side::Customer,
        &actor,
        &entry,
        &ctx.business_day(),
        ctx.now,
    )?;
    if let (Some(share), Some(session)) = (versement_share(&posted, &q), &session) {
        cash::book(tx, ctx, &session.id, &share)?;
    }
    let allocations = ledger::allocations_of(tx, Side::Customer, &posted.id)?;
    audit::append(
        tx,
        &actor.event(
            "credit.collected",
            "customer",
            Some(&input.customer_id),
            json!({
                "entryId": posted.id,
                "receiptNumber": receipt.number,
                "amount": tender.amount,
                "method": tender.method,
                "stampDuty": q.stamp_duty,
                "stampCharged": q.stamp_charged,
                "balanceAfter": posted.balance_after,
                "allocations": allocations
                    .iter()
                    .map(|a| json!({ "debitId": a.debit_id, "amount": a.amount }))
                    .collect::<Vec<_>>(),
            }),
        ),
    )?;
    Ok(Collected {
        customer: super::get(tx, &input.customer_id)?,
        entry: posted,
        quote: q,
        allocations,
    })
}

/// Records a DUPLICATA of a versement receipt (SAL-056 for kridi) on the customer's history,
/// before it prints; returns the versement. Only a versement has a receipt.
pub fn record_reprint(
    tx: &Transaction<'_>,
    actor: &Actor,
    entry_id: &str,
) -> Result<LedgerEntry, AppError> {
    let entry = ledger::get(tx, Side::Customer, entry_id)?;
    let number = match (&entry.kind, &entry.receipt_number) {
        (EntryKind::Payment, Some(number)) => number.clone(),
        _ => return Err(AppError::not_found("ledger_entry")),
    };
    audit::append(
        tx,
        &actor.event(
            "credit.receipt_reprinted",
            "customer",
            Some(&entry.party_id),
            json!({ "entryId": entry.id, "receiptNumber": number }),
        ),
    )?;
    Ok(entry)
}

/// The account tenders of a sale, checked against the customer (PAY-011/013/020, CRD-001/002,
/// CRM-005). Nothing is written here: `book_sale` posts them once the ticket is validated.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct AccountTenders {
    pub customer: Option<super::Customer>,
    pub credit: Money,
    pub voucher: Money,
}

pub(crate) fn check_account_tenders(
    conn: &Connection,
    ctx: &TillContext,
    customer_id: Option<&str>,
    tenders: &[PaymentInput],
) -> Result<AccountTenders, AppError> {
    let sum = |method: PaymentMethod| {
        Money::checked_sum(
            tenders
                .iter()
                .filter(|t| t.method == method)
                .map(|t| t.amount),
        )
        .ok_or_else(|| AppError::validation("payments", "amount_invalid"))
    };
    let credit = sum(PaymentMethod::Credit)?;
    // Only store-credit vouchers spend the customer's account; a voucher tender naming an order,
    // a service ticket or a gift voucher spends that register (`vouchers::book`).
    let voucher = crate::vouchers::store_credit_total(tenders)?;
    let customer = customer_id
        .map(|id| super::selectable(conn, id))
        .transpose()?;
    let cheque_needs_customer = tenders.iter().any(|t| t.method == PaymentMethod::Cheque)
        && crate::settings::get::<bool>(conn, "payments.cheque_requires_customer")?;
    if customer.is_none()
        && (credit.is_positive() || voucher.is_positive() || cheque_needs_customer)
    {
        return Err(AppError::validation("customerId", "customer_required"));
    }
    // Store credit first: the limit below counts the voucher, so it must be credit he has.
    if let (true, Some(c)) = (voucher.is_positive(), &customer) {
        let available = c.store_credit;
        if voucher > available {
            return Err(AppError::rejected(
                json!({ "available": available }),
                "store_credit_insufficient",
            ));
        }
    }
    if let (true, Some(c)) = (credit.is_positive(), &customer) {
        if !ctx.allows(Permission::CreditView) {
            return Err(AppError::Permission {
                permission: Permission::CreditView.as_str().to_owned(),
            });
        }
        if c.is_blocked {
            return Err(AppError::conflict("customer_blocked"));
        }
        if let Some(limit) = c.credit_limit {
            // The balance once the ticket is booked: the voucher spends store credit (a
            // negative balance) and the kridi adds to it, both on the same account.
            let after = c
                .balance
                .checked_add(credit)
                .and_then(|b| b.checked_add(voucher))
                .ok_or_else(|| AppError::validation("payments", "amount_invalid"))?;
            if after > limit && !ctx.allows(Permission::CreditLimitOverride) {
                return Err(AppError::rejected(
                    json!({ "customerId": c.id, "balance": c.balance, "limit": limit, "after": after }),
                    "credit_limit_exceeded",
                ));
            }
        }
    }
    Ok(AccountTenders {
        customer,
        credit,
        voucher,
    })
}

/// Posts the account tenders of a validated ticket; the customer's balance after them.
pub(crate) fn book_sale(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    account: &AccountTenders,
    document_id: &str,
) -> Result<Option<Money>, AppError> {
    let Some(customer) = &account.customer else {
        return Ok(None);
    };
    let actor = ctx.actor();
    let day = ctx.business_day();
    for (kind, amount) in [
        (EntryKind::SaleCredit, account.credit),
        (EntryKind::VoucherRedeemed, account.voucher),
    ] {
        if amount.is_positive() {
            let entry =
                NewEntry::new(&customer.id, kind, amount).referencing("document", document_id);
            ledger::append(tx, Side::Customer, &actor, &entry, &day, ctx.now)?;
        }
    }
    Ok(Some(super::get(tx, &customer.id)?.balance))
}

/// A return refunded to the account (RET-007): kridi reduced or store credit given.
pub(crate) fn book_refund(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    customer_id: &str,
    refund: Money,
    method: PaymentMethod,
    document_id: &str,
) -> Result<Money, AppError> {
    active_customer(tx, customer_id)?;
    let mut entry = NewEntry::new(
        customer_id,
        EntryKind::Refund,
        refund
            .checked_neg()
            .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?,
    )
    .referencing("document", document_id);
    entry.method = Some(method);
    let posted = ledger::append(
        tx,
        Side::Customer,
        &ctx.actor(),
        &entry,
        &ctx.business_day(),
        ctx.now,
    )?;
    Ok(posted.balance_after)
}

/// Unsettled debts, oldest first: the list a versement can target.
pub fn open_items(conn: &Connection, customer_id: &str) -> Result<Vec<LedgerEntry>, AppError> {
    super::get(conn, customer_id)?;
    ledger::open_debits(conn, Side::Customer, customer_id)
}

pub fn entries(
    conn: &Connection,
    customer_id: &str,
    q: &EntryQuery,
) -> Result<EntryPage, AppError> {
    super::get(conn, customer_id)?;
    ledger::entries(conn, Side::Customer, customer_id, q)
}

pub fn statement(
    conn: &Connection,
    customer_id: &str,
    from: &str,
    to: &str,
) -> Result<Statement, AppError> {
    ledger::statement(conn, Side::Customer, customer_id, from, to)
}

pub fn aging(conn: &Connection, as_of: &str) -> Result<Vec<AgingRow>, AppError> {
    ledger::aging(conn, Side::Customer, as_of)
}

/// Total the customers owe the shop (dashboard, RPT-001).
pub fn receivables(conn: &Connection) -> Result<Money, AppError> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(balance), 0) FROM customers WHERE balance > 0",
        [],
        |r| r.get(0),
    )?)
}

pub fn rebuild_balances(tx: &Transaction<'_>, actor: &Actor) -> Result<LedgerRebuild, AppError> {
    ledger::rebuild_balances(tx, Side::Customer, actor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::shop;

    #[test]
    fn the_paper_notebook_opens_once_and_adjustments_need_a_reason() {
        let shop = shop();
        let karim = shop.customer("Karim");
        let ctx = shop.owner_till();
        let id = karim.id.clone();
        let opening = shop
            .db
            .write(move |tx| set_opening(tx, &ctx, &id, Money(1_250_000), Some("carnet")))
            .unwrap();
        assert_eq!(
            (opening.kind, opening.balance_after),
            (EntryKind::Opening, Money(1_250_000))
        );
        let ctx = shop.owner_till();
        let id = karim.id.clone();
        assert_eq!(
            shop.db
                .write(move |tx| set_opening(tx, &ctx, &id, Money(10), None))
                .unwrap_err(),
            AppError::conflict("opening_already_set")
        );
        let ctx = shop.owner_till();
        let id = karim.id.clone();
        assert_eq!(
            shop.db
                .write(move |tx| adjust(tx, &ctx, &id, Money(-50_000), " "))
                .unwrap_err(),
            AppError::validation("note", "required")
        );
        let ctx = shop.owner_till();
        let id = karim.id.clone();
        let fixed = shop
            .db
            .write(move |tx| adjust(tx, &ctx, &id, Money(-50_000), "erreur de report"))
            .unwrap();
        assert_eq!(fixed.balance_after, Money(1_200_000));
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action IN ('credit.opening_set', 'credit.adjusted')"),
            2
        );
    }

    use crate::payments::PaymentMethod;

    fn karim_owing(shop: &crate::testkit::Shop, amount: i64) -> super::super::Customer {
        let karim = shop.customer("Karim");
        let ctx = shop.owner_till();
        let id = karim.id.clone();
        shop.db
            .write(move |tx| set_opening(tx, &ctx, &id, Money(amount), None))
            .unwrap();
        karim
    }

    fn input(customer: &str, amount: i64, method: PaymentMethod) -> CollectInput {
        CollectInput {
            customer_id: customer.into(),
            amount: Money(amount),
            method,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
            allocate_to: Vec::new(),
            note: None,
        }
    }

    #[test]
    fn a_cash_versement_pays_the_stamp_duty_on_the_cash_it_brings() {
        let shop = shop();
        let karim = karim_owing(&shop, 5_000_000);
        let session = shop.open_till(0);
        let ctx = shop.till();
        let q = input(&karim.id, 4_500_000, PaymentMethod::Cash);
        let quoted = shop
            .db
            .read({
                let ctx = ctx.clone();
                let q = q.clone();
                move |c| quote(c, &ctx, &q)
            })
            .unwrap();
        // 45 000 DA in cash: 450 tranches of 100 DA at 1.5 DA = 675 DA (bracket 30 000–100 000).
        assert_eq!(
            (quoted.stamp_duty, quoted.stamp_charged, quoted.to_receive),
            (Money(67_500), Money(67_500), Money(4_567_500))
        );
        let done = shop.db.write(move |tx| collect(tx, &ctx, q)).unwrap();
        assert_eq!(done.entry.receipt_number.as_deref(), Some("RV2026-000001"));
        assert_eq!(
            (done.entry.amount, done.entry.balance_after),
            (Money(-4_500_000), Money(500_000))
        );
        assert_eq!(
            done.entry.cash_session_id.as_deref(),
            Some(session.id.as_str())
        );
        assert_eq!(
            shop.count("SELECT SUM(amount) FROM cash_movements WHERE kind = 'credit_payment'"),
            4_567_500,
            "the drawer receives the versement and the duty"
        );
        assert_eq!(done.allocations.len(), 1, "settles the opening debt");
    }

    #[test]
    fn a_card_or_detailed_cheque_versement_pays_no_duty() {
        let shop = shop();
        let karim = karim_owing(&shop, 1_000_000);
        let ctx = shop.till();
        let mut card = input(&karim.id, 400_000, PaymentMethod::CardCib);
        card.reference = Some("AUT 482913".into());
        let done = shop.db.write(move |tx| collect(tx, &ctx, card)).unwrap();
        assert_eq!(done.quote.stamp_duty, Money::ZERO);
        assert_eq!(
            done.entry.cash_session_id, None,
            "no till open: nothing in a drawer"
        );
        let ctx = shop.till();
        let mut cheque = input(&karim.id, 300_000, PaymentMethod::Cheque);
        cheque.reference = Some("0012345".into());
        cheque.cheque_bank = Some("BNA".into());
        cheque.cheque_date = Some("2026-09-28".into());
        let done = shop.db.write(move |tx| collect(tx, &ctx, cheque)).unwrap();
        assert_eq!(done.quote.stamp_duty, Money::ZERO);
        let ctx = shop.till();
        let bare_cheque = input(&karim.id, 1_000, PaymentMethod::Cheque);
        assert_eq!(
            shop.db
                .write(move |tx| collect(tx, &ctx, bare_cheque))
                .unwrap_err(),
            AppError::validation("reference", "cheque_details_required")
        );
        let ctx = shop.till();
        let credit = input(&karim.id, 1_000, PaymentMethod::Credit);
        assert_eq!(
            shop.db
                .write(move |tx| collect(tx, &ctx, credit))
                .unwrap_err(),
            AppError::validation("method", "payment_method_invalid")
        );
    }

    #[test]
    fn an_absorbed_duty_is_recorded_but_not_collected() {
        let shop = shop();
        shop.set("stamp_duty.mode", serde_json::json!("absorb"));
        let karim = karim_owing(&shop, 1_000_000);
        shop.open_till(0);
        let ctx = shop.till();
        let done = shop
            .db
            .write(move |tx| collect(tx, &ctx, input(&karim.id, 1_000_000, PaymentMethod::Cash)))
            .unwrap();
        assert_eq!(
            (done.entry.stamp_duty, done.entry.stamp_charged),
            (Money(10_000), Money::ZERO)
        );
        assert_eq!(
            shop.count("SELECT SUM(amount) FROM cash_movements WHERE kind = 'credit_payment'"),
            1_000_000
        );
    }

    #[test]
    fn cash_needs_an_open_till_and_versements_can_run_ahead_of_the_debt() {
        let shop = shop();
        let karim = karim_owing(&shop, 200_000);
        let ctx = shop.till();
        let cash = input(&karim.id, 300_000, PaymentMethod::Cash);
        assert_eq!(
            shop.db
                .write({
                    let cash = cash.clone();
                    move |tx| collect(tx, &ctx, cash)
                })
                .unwrap_err()
                .code(),
            "cash_session_closed"
        );
        shop.open_till(0);
        let ctx = shop.till();
        let done = shop.db.write(move |tx| collect(tx, &ctx, cash)).unwrap();
        assert_eq!(
            done.entry.balance_after,
            Money(-100_000),
            "an advance (acompte, CRD-007)"
        );
        assert_eq!(done.customer.store_credit, Money(100_000));
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'credit.collected'"),
            1
        );
    }

    /// The directory and the customer's file show what is left under the limit and the store
    /// credit as the backend computes them (the UI computes no money).
    #[test]
    fn a_customer_carries_what_he_can_still_buy_on_credit_and_his_store_credit() {
        let shop = shop();
        let karim = karim_owing(&shop, 300_000);
        let pack = std::sync::Arc::clone(&shop.pack);
        let actor = shop.owner.actor();
        let id = karim.id.clone();
        shop.db
            .write(move |tx| {
                let mut input = super::super::CustomerInput::named("Karim");
                input.credit_limit = Some(Money(500_000));
                super::super::update(tx, &actor, &pack, &id, input)
            })
            .unwrap();
        shop.customer("Nadia");
        let figures = |shop: &crate::testkit::Shop| {
            let id = karim.id.clone();
            let customer = shop.db.read(move |c| super::super::get(c, &id)).unwrap();
            let hits = shop
                .db
                .read(|c| super::super::search(c, &super::super::CustomerQuery::default()))
                .unwrap();
            let hit = |name: &str| {
                let h = hits.iter().find(|h| h.name == name).unwrap();
                (h.available_credit, h.store_credit)
            };
            (
                (customer.available_credit, customer.store_credit),
                hit("Karim"),
                hit("Nadia"),
            )
        };
        assert_eq!(
            figures(&shop),
            (
                (Some(Money(200_000)), Money::ZERO),
                (Some(Money(200_000)), Money::ZERO),
                (None, Money::ZERO)
            ),
            "3 000 DA owed on a 5 000 DA limit; no limit, nothing to show"
        );
        let ctx = shop.till();
        let mut card = input(&karim.id, 400_000, PaymentMethod::CardCib);
        card.reference = Some("AUT 482913".into());
        let done = shop.db.write(move |tx| collect(tx, &ctx, card)).unwrap();
        assert_eq!(
            (done.customer.available_credit, done.customer.store_credit),
            (Some(Money(500_000)), Money(100_000))
        );
        assert_eq!(
            figures(&shop),
            (
                (Some(Money(500_000)), Money(100_000)),
                (Some(Money(500_000)), Money(100_000)),
                (None, Money::ZERO)
            ),
            "paid 1 000 DA ahead: the whole limit and 1 000 DA of store credit"
        );
    }

    #[test]
    fn a_versement_receipt_reprint_is_journaled_and_only_a_versement_has_one() {
        let shop = shop();
        let karim = karim_owing(&shop, 300_000);
        let ctx = shop.till();
        let mut card = input(&karim.id, 100_000, PaymentMethod::CardCib);
        card.reference = Some("AUT 482913".into());
        let done = shop.db.write(move |tx| collect(tx, &ctx, card)).unwrap();
        let actor = shop.cashier.actor();
        let id = done.entry.id.clone();
        let reprinted = shop
            .db
            .write(move |tx| record_reprint(tx, &actor, &id))
            .unwrap();
        assert_eq!(reprinted.receipt_number.as_deref(), Some("RV2026-000001"));
        let (entity, payload): (String, String) = shop
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT entity_id, payload FROM audit_log
                     WHERE action = 'credit.receipt_reprinted'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?)
            })
            .unwrap();
        assert_eq!(entity, karim.id, "on the customer's history");
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(
            payload,
            json!({ "entryId": done.entry.id, "receiptNumber": "RV2026-000001" })
        );
        let opening = shop
            .db
            .read(move |c| entries(c, &karim.id, &EntryQuery::default()))
            .unwrap()
            .entries
            .into_iter()
            .find(|e| e.kind == EntryKind::Opening)
            .unwrap();
        let actor = shop.cashier.actor();
        assert_eq!(
            shop.db
                .write(move |tx| record_reprint(tx, &actor, &opening.id))
                .unwrap_err(),
            AppError::not_found("ledger_entry"),
            "an opening balance has no receipt to reprint"
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'credit.receipt_reprinted'"),
            1
        );
    }

    #[test]
    fn statements_and_ageing_read_the_ledger() {
        let shop = shop();
        let karim = karim_owing(&shop, 700_000);
        let ctx = shop.till();
        let mut card = input(&karim.id, 200_000, PaymentMethod::Edahabia);
        card.reference = Some("TX-99".into());
        shop.db.write(move |tx| collect(tx, &ctx, card)).unwrap();
        let s = shop
            .db
            .read(move |c| statement(c, &karim.id, "2026-09-01", "2026-09-30"))
            .unwrap();
        assert_eq!(
            (s.opening, s.debits, s.credits, s.closing),
            (Money::ZERO, Money(700_000), Money(200_000), Money(500_000))
        );
        let rows = shop.db.read(|c| aging(c, "2026-09-28")).unwrap();
        assert_eq!(rows[0].current, Money(500_000));
        assert_eq!(shop.db.read(receivables).unwrap(), Money(500_000));
    }
}
