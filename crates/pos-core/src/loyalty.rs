//! Loyalty points (LOY-002 lite): an enrolled customer (a live card, the `loyalty` consent logged)
//! earns `loyalty.points_per_100_da` points per whole 100 DA of a ticket not paid by vouchers;
//! returned goods take their points back (never below zero: points already spent stay spent);
//! earned points expire after `loyalty.points_validity_months`, oldest first; a reward is points
//! turned into a loyalty voucher (Task 13 `gift_vouchers`), never a draw or a prize (PRM-025).
//!
//! LAN: points are a ledger sum, so `redeem`, `adjust` and `expire_due` are single
//! functions the app registers as host services; `earn` and `reverse` run inside the sale
//! and the return that move them.

use crate::audit::{self, Actor};
use crate::auth::Permission;
use crate::documents::Document;
use crate::error::AppError;
use crate::gift_vouchers::{self, Voucher, VoucherKind};
use crate::ids::new_id;
use crate::money::Money;
use crate::packs::{self, Module};
use crate::payments::PaymentMethod;
use crate::sqlenum::sql_text_enum;
use crate::till::TillContext;
use chrono::{Days, Months, NaiveDate};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LoyaltyCard {
    pub code: String,
    pub customer_id: String,
    pub issued_at: i64,
    pub blocked_at: Option<i64>,
}

/// `Deserialize`: the main PC's host service answers a till with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub customer_id: String,
    /// The live card; `None` when the customer is not (or no longer) enrolled.
    pub card: Option<LoyaltyCard>,
    pub balance: i64,
    /// Earned points not yet used that expire within 30 days.
    pub expiring_soon: i64,
    /// The first expiry date of those points.
    pub next_expiry: Option<String>,
    /// What one point is worth in a loyalty voucher.
    pub point_value: Money,
    /// The balance reaches `loyalty.min_redeem_points` and the card is live.
    pub redeemable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PointsKind {
    Earn,
    Redeem,
    Expire,
    Reversal,
    Adjust,
}

impl PointsKind {
    pub const ALL: [PointsKind; 5] = [
        PointsKind::Earn,
        PointsKind::Redeem,
        PointsKind::Expire,
        PointsKind::Reversal,
        PointsKind::Adjust,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            PointsKind::Earn => "earn",
            PointsKind::Redeem => "redeem",
            PointsKind::Expire => "expire",
            PointsKind::Reversal => "reversal",
            PointsKind::Adjust => "adjust",
        }
    }

    pub fn parse(s: &str) -> Option<PointsKind> {
        PointsKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(PointsKind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PointsEntry {
    pub kind: PointsKind,
    /// Signed: in (earn, positive adjustment) or out.
    pub points: i64,
    /// The ticket that earned them or the return note that took them back.
    pub document_number: Option<String>,
    /// The loyalty voucher a redemption issued.
    pub voucher_code: Option<String>,
    pub expires_on: Option<String>,
    pub business_date: String,
    /// The reason of an adjustment.
    pub note: Option<String>,
}

const CARD: &str = "SELECT code, customer_id, issued_at, blocked_at FROM loyalty_cards";

fn card_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<LoyaltyCard> {
    Ok(LoyaltyCard {
        code: r.get(0)?,
        customer_id: r.get(1)?,
        issued_at: r.get(2)?,
        blocked_at: r.get(3)?,
    })
}

fn live_card(conn: &Connection, customer_id: &str) -> Result<Option<LoyaltyCard>, AppError> {
    Ok(conn
        .prepare_cached(&format!(
            "{CARD} WHERE customer_id = ?1 AND blocked_at IS NULL"
        ))?
        .query_row([customer_id], card_of)
        .optional()?)
}

fn card_exists(conn: &Connection, code: &str) -> Result<bool, AppError> {
    Ok(conn
        .prepare_cached("SELECT EXISTS (SELECT 1 FROM loyalty_cards WHERE code = ?1)")?
        .query_row([code], |r| r.get(0))?)
}

/// A card code for a customer with none typed: an in-store EAN-13 `297…` not used yet.
fn generated_code(tx: &Transaction<'_>) -> Result<String, AppError> {
    let mut n: i64 = tx.query_row("SELECT COUNT(*) + 1 FROM loyalty_cards", [], |r| r.get(0))?;
    loop {
        let body = format!("297{n:09}");
        let check = crate::labels::build::gtin_check_digit(&body)
            .filter(|_| body.len() == 12)
            .ok_or_else(|| AppError::validation("code", "loyalty_card_invalid"))?;
        let code = format!("{body}{check}");
        if !card_exists(tx, &code)? {
            return Ok(code);
        }
        n = n
            .checked_add(1)
            .ok_or_else(|| AppError::validation("code", "loyalty_card_invalid"))?;
    }
}

/// Enrols a customer: a card code typed (8 to 20 letters or digits) or generated, one live card
/// per customer (an enrolled customer gets his card back), the `loyalty` consent logged. One
/// `loyalty.enrolled` row.
pub fn enrol(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    customer_id: &str,
    code: Option<String>,
) -> Result<LoyaltyCard, AppError> {
    packs::require(tx, Module::Loyalty)?;
    crate::customers::selectable(tx, customer_id)?;
    if let Some(card) = live_card(tx, customer_id)? {
        return Ok(card);
    }
    let code = match code
        .map(|c| c.trim().to_uppercase())
        .filter(|c| !c.is_empty())
    {
        Some(c)
            if (8..=20).contains(&c.len())
                && c.bytes().all(|b| b.is_ascii_alphanumeric())
                && !card_exists(tx, &c)? =>
        {
            c
        }
        Some(_) => return Err(AppError::validation("code", "loyalty_card_invalid")),
        None => generated_code(tx)?,
    };
    tx.execute(
        "INSERT INTO loyalty_cards (code, customer_id, issued_at) VALUES (?1, ?2, ?3)",
        params![code, customer_id, ctx.now],
    )?;
    tx.execute(
        "INSERT INTO customer_consents (id, customer_id, channel, granted, source, created_at, user_id)
         VALUES (?1, ?2, 'loyalty', 1, 'counter', ?3, ?4)",
        params![new_id(), customer_id, ctx.now, ctx.session.user_id],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "loyalty.enrolled",
            "customer",
            Some(customer_id),
            json!({ "card": code }),
        ),
    )?;
    live_card(tx, customer_id)?.ok_or_else(|| AppError::validation("code", "loyalty_card_invalid"))
}

/// Blocks a live card (lost or stolen): the customer keeps his points and can be enrolled with a
/// new card. One `loyalty.card_blocked` row.
pub fn block(tx: &Transaction<'_>, ctx: &TillContext, code: &str) -> Result<LoyaltyCard, AppError> {
    let code = code.trim().to_uppercase();
    let card = tx
        .prepare_cached(&format!("{CARD} WHERE code = ?1 AND blocked_at IS NULL"))?
        .query_row([&code], card_of)
        .optional()?
        .ok_or_else(|| AppError::validation("code", "loyalty_card_invalid"))?;
    tx.execute(
        "UPDATE loyalty_cards SET blocked_at = ?2 WHERE code = ?1",
        params![code, ctx.now],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "loyalty.card_blocked",
            "customer",
            Some(&card.customer_id),
            json!({ "card": code }),
        ),
    )?;
    Ok(LoyaltyCard {
        blocked_at: Some(ctx.now),
        ..card
    })
}

/// The customer of a live card (the till's scanner asks here before the product search).
pub fn customer_of_card(conn: &Connection, code: &str) -> Result<Option<String>, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT customer_id FROM loyalty_cards WHERE code = ?1 AND blocked_at IS NULL",
        )?
        .query_row([code.trim().to_uppercase()], |r| r.get(0))
        .optional()?)
}

fn setting(conn: &Connection, key: &'static str) -> Result<i64, AppError> {
    crate::settings::get(conn, key)
}

fn day_text(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn points_expiry(conn: &Connection, day: NaiveDate) -> Result<Option<String>, AppError> {
    let months = setting(conn, "loyalty.points_validity_months")?;
    Ok(u32::try_from(months)
        .ok()
        .filter(|m| *m > 0)
        .and_then(|m| day.checked_add_months(Months::new(m)))
        .map(day_text))
}

fn points_balance(conn: &Connection, customer_id: &str) -> Result<i64, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT COALESCE(SUM(points), 0) FROM loyalty_ledger WHERE customer_id = ?1",
        )?
        .query_row([customer_id], |r| r.get(0))?)
}

/// The points of a ticket of `total_ttc` paid by `payments`: `loyalty.points_per_100_da` per whole
/// 100 DA not paid by vouchers (their points were earned or given when they were issued).
fn points_on(
    conn: &Connection,
    total_ttc: Money,
    payments: impl Iterator<Item = (PaymentMethod, Money)>,
) -> Result<i64, AppError> {
    let vouchers = Money::checked_sum(
        payments
            .filter(|(method, _)| *method == PaymentMethod::Voucher)
            .map(|(_, amount)| amount),
    )
    .ok_or_else(|| AppError::validation("payments", "amount_invalid"))?;
    let base = total_ttc
        .checked_sub(vouchers)
        .unwrap_or(Money::ZERO)
        .max(Money::ZERO);
    Ok((base.0 / 10_000).saturating_mul(setting(conn, "loyalty.points_per_100_da")?))
}

/// The points the ticket being paid earns `customer_id` (the payment dialog's line, LOY-002), by
/// the rule of the earn row the sale writes; `None` while the module is off or when the customer
/// has no live card (no points line).
pub fn preview_points(
    conn: &Connection,
    customer_id: &str,
    total_ttc: Money,
    payments: &[crate::payments::PaymentInput],
) -> Result<Option<i64>, AppError> {
    if !packs::enabled(conn, Module::Loyalty)? || live_card(conn, customer_id)?.is_none() {
        return Ok(None);
    }
    points_on(
        conn,
        total_ttc,
        payments.iter().map(|p| (p.method, p.amount)),
    )
    .map(Some)
}

/// Points of a validated ticket: on what was not paid by vouchers (their points were earned or
/// given when they were issued), for an enrolled customer, module on. The vouchers are read from
/// the ticket's validated payments, so an exchange's store credit (its goods already earned their
/// points on the returned ticket) earns nothing again. Journaled by the sale.
pub(crate) fn earn(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    document: &Document,
) -> Result<i64, AppError> {
    let Some(customer_id) = document.customer_id.as_deref() else {
        return Ok(0);
    };
    if !packs::enabled(tx, Module::Loyalty)? || live_card(tx, customer_id)?.is_none() {
        return Ok(0);
    }
    let points = points_on(
        tx,
        document.total_ttc,
        document.payments.iter().map(|p| (p.method, p.amount)),
    )?;
    if points > 0 {
        tx.execute(
            "INSERT INTO loyalty_ledger (id, customer_id, kind, points, document_id, expires_on, business_date,
                                         created_at, user_id, terminal_id)
             VALUES (?1, ?2, 'earn', ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                new_id(),
                customer_id,
                points,
                document.id,
                points_expiry(tx, ctx.business_date())?,
                ctx.business_day(),
                ctx.now,
                ctx.session.user_id,
                ctx.session.terminal_id
            ],
        )?;
    }
    Ok(points)
}

/// How many exchanges back `reverse` follows a store credit (a guard; real chains are short).
const EXCHANGE_CHAIN_LIMIT: u32 = 32;

/// Takes back the points `refund` earned on `original_document_id`, pro rata of that ticket's
/// earned points, capped by what it earned minus what earlier returns took and by the balance.
/// When that ticket was an exchange's new ticket, the part of `refund` its store credit paid
/// takes back, the same way, the points the exchanged goods earned on the ticket before it (they
/// were carried through the exchange, never earned again). Journaled by the return.
pub(crate) fn reverse(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    original_document_id: &str,
    refund: Money,
    return_document_id: &str,
) -> Result<i64, AppError> {
    let mut ticket = original_document_id.to_owned();
    let mut refund = refund;
    let mut back = 0_i64;
    for _ in 0..EXCHANGE_CHAIN_LIMIT {
        back = back.saturating_add(reverse_own(tx, ctx, &ticket, refund, return_document_id)?);
        let Some((earlier, part)) = exchange_credit_part(tx, &ticket, refund)? else {
            break;
        };
        ticket = earlier;
        refund = part;
    }
    Ok(back)
}

/// Whether a return of `original_document_id` takes points back: it (or the ticket an exchange
/// paid it from) earned some (such a return is made on the main PC).
pub(crate) fn earned_on(conn: &Connection, original_document_id: &str) -> Result<bool, AppError> {
    let mut ticket = original_document_id.to_owned();
    for _ in 0..EXCHANGE_CHAIN_LIMIT {
        let earned: bool = conn
            .prepare_cached(
                "SELECT EXISTS (SELECT 1 FROM loyalty_ledger WHERE document_id = ?1 AND kind = 'earn')",
            )?
            .query_row([&ticket], |r| r.get(0))?;
        if earned {
            return Ok(true);
        }
        match exchange_credit_part(conn, &ticket, Money(1_000_000_000_000))? {
            Some((earlier, _)) => ticket = earlier,
            None => break,
        }
    }
    Ok(false)
}

/// For an exchange's new ticket: the ticket the exchanged goods came from, and the part of
/// `refund` paid by the exchange's store credit (the voucher row naming the exchange's return).
fn exchange_credit_part(
    tx: &Connection,
    ticket_id: &str,
    refund: Money,
) -> Result<Option<(String, Money)>, AppError> {
    if !refund.is_positive() {
        return Ok(None);
    }
    let row: Option<(String, i64, i64)> = tx
        .prepare_cached(
            "SELECT r.parent_id, p.amount, t.total_ttc
             FROM documents t
             JOIN documents r ON r.id = t.parent_id AND r.doc_type = 'return_note' AND r.parent_id IS NOT NULL
             JOIN payments p ON p.document_id = t.id AND p.method = 'voucher' AND p.reference = r.number
             WHERE t.id = ?1",
        )?
        .query_row([ticket_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?;
    let Some((earlier, credit, total)) = row else {
        return Ok(None);
    };
    if total <= 0 || credit <= 0 {
        return Ok(None);
    }
    let part = i64::try_from(i128::from(refund.0) * i128::from(credit) / i128::from(total))
        .unwrap_or(credit)
        .min(credit);
    Ok((part > 0).then_some((earlier, Money(part))))
}

/// `reverse` on one ticket's own earned points.
fn reverse_own(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    original_document_id: &str,
    refund: Money,
    return_document_id: &str,
) -> Result<i64, AppError> {
    let row: Option<(String, i64, i64)> = tx
        .query_row(
            "SELECT e.customer_id, e.points, d.total_ttc FROM loyalty_ledger e JOIN documents d ON d.id = e.document_id
             WHERE e.document_id = ?1 AND e.kind = 'earn'",
            [original_document_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((customer_id, earned, total)) = row else {
        return Ok(0);
    };
    let taken: i64 = tx.query_row(
        "SELECT COALESCE(-SUM(points), 0) FROM loyalty_ledger
         WHERE customer_id = ?1 AND kind = 'reversal' AND note = ?2",
        params![customer_id, original_document_id],
        |r| r.get(0),
    )?;
    let balance = points_balance(tx, &customer_id)?;
    let share = if total > 0 && refund.is_positive() {
        i64::try_from(i128::from(earned) * i128::from(refund.0) / i128::from(total))
            .unwrap_or(earned)
    } else {
        0
    };
    let back = share.min(earned.saturating_sub(taken)).min(balance);
    if back > 0 {
        tx.execute(
            "INSERT INTO loyalty_ledger (id, customer_id, kind, points, document_id, note, business_date, created_at,
                                         user_id, terminal_id)
             VALUES (?1, ?2, 'reversal', ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                new_id(),
                customer_id,
                -back,
                return_document_id,
                original_document_id,
                ctx.business_day(),
                ctx.now,
                ctx.session.user_id,
                ctx.session.terminal_id
            ],
        )?;
    }
    Ok(back.max(0))
}

/// The earned rows of a customer, oldest expiry first, each with what the points that left the
/// account (redeemed, expired, taken back, adjusted down) did not consume yet: points leave
/// oldest first.
fn alive_earnings(conn: &Connection, customer_id: &str) -> Result<Vec<(String, i64)>, AppError> {
    let used: i64 = conn
        .prepare_cached(
            "SELECT COALESCE(-SUM(points), 0) FROM loyalty_ledger
             WHERE customer_id = ?1 AND (kind IN ('redeem', 'expire', 'reversal') OR (kind = 'adjust' AND points < 0))",
        )?
        .query_row([customer_id], |r| r.get(0))?;
    let earned = conn
        .prepare_cached(
            "SELECT expires_on, points FROM loyalty_ledger
             WHERE customer_id = ?1 AND kind = 'earn' AND expires_on IS NOT NULL
             ORDER BY expires_on, created_at, rowid",
        )?
        .query_map([customer_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut left_to_use = used.max(0);
    let mut out = Vec::new();
    for (expires_on, points) in earned {
        let consumed = points.min(left_to_use);
        left_to_use -= consumed;
        if points > consumed {
            out.push((expires_on, points - consumed));
        }
    }
    Ok(out)
}

/// The customer's points as of `today`.
pub fn account(conn: &Connection, customer_id: &str, today: &str) -> Result<Account, AppError> {
    let card = live_card(conn, customer_id)?;
    let balance = points_balance(conn, customer_id)?;
    let soon = NaiveDate::parse_from_str(today, "%Y-%m-%d")
        .map_err(|_| AppError::validation("today", "date_invalid"))?
        .checked_add_days(Days::new(30))
        .map(day_text)
        .ok_or_else(|| AppError::validation("today", "date_invalid"))?;
    let expiring: Vec<(String, i64)> = alive_earnings(conn, customer_id)?
        .into_iter()
        .filter(|(on, _)| *on <= soon)
        .collect();
    let expiring_soon = expiring
        .iter()
        .fold(0_i64, |a, (_, p)| a.saturating_add(*p))
        .min(balance.max(0));
    let point_value = Money(setting(conn, "loyalty.point_value")?);
    let minimum = setting(conn, "loyalty.min_redeem_points")?;
    Ok(Account {
        customer_id: customer_id.to_owned(),
        redeemable: card.is_some() && balance >= minimum,
        card,
        balance,
        expiring_soon,
        next_expiry: expiring.first().map(|(on, _)| on.clone()),
        point_value,
    })
}

/// The points a ticket earned and its customer's balance (the receipt's loyalty line, LOY-002).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPoints {
    pub customer_id: String,
    pub earned: i64,
    /// The customer's points now (a reprint shows the balance of the day it is printed).
    pub balance: i64,
    /// The customer's live card; `None` once it was blocked.
    pub card: Option<String>,
}

/// The earn row of the ticket `document_id`, with its customer's balance and card; `None` for a
/// ticket that earned nothing.
pub fn points_of_document(
    conn: &Connection,
    document_id: &str,
) -> Result<Option<DocumentPoints>, AppError> {
    let earned: Option<(String, i64)> = conn
        .prepare_cached(
            "SELECT customer_id, SUM(points) FROM loyalty_ledger
             WHERE document_id = ?1 AND kind = 'earn' GROUP BY customer_id",
        )?
        .query_row([document_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let Some((customer_id, earned)) = earned else {
        return Ok(None);
    };
    Ok(Some(DocumentPoints {
        balance: points_balance(conn, &customer_id)?,
        card: live_card(conn, &customer_id)?.map(|c| c.code),
        customer_id,
        earned,
    }))
}

/// The customer's ledger, newest first, with the ticket or return number and the voucher code.
pub fn entries(conn: &Connection, customer_id: &str) -> Result<Vec<PointsEntry>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT l.kind, l.points, d.number, v.code, l.expires_on, l.business_date,
                CASE WHEN l.kind = 'adjust' THEN l.note END
         FROM loyalty_ledger l
         LEFT JOIN documents d ON d.id = l.document_id
         LEFT JOIN gift_vouchers v ON v.id = l.voucher_id
         WHERE l.customer_id = ?1
         ORDER BY l.created_at DESC, l.rowid DESC",
    )?;
    let rows = stmt
        .query_map([customer_id], |r| {
            Ok(PointsEntry {
                kind: r.get(0)?,
                points: r.get(1)?,
                document_number: r.get(2)?,
                voucher_code: r.get(3)?,
                expires_on: r.get(4)?,
                business_date: r.get(5)?,
                note: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn require_enrolled(conn: &Connection, customer_id: &str) -> Result<(), AppError> {
    if live_card(conn, customer_id)?.is_none() {
        return Err(AppError::validation("customerId", "loyalty_not_enrolled"));
    }
    Ok(())
}

/// Turns `points` (at least `loyalty.min_redeem_points`) into a loyalty voucher worth
/// `points × loyalty.point_value`, named for the customer. One `loyalty.redeemed` row.
pub fn redeem(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    customer_id: &str,
    points: i64,
) -> Result<Voucher, AppError> {
    packs::require(tx, Module::Loyalty)?;
    require_enrolled(tx, customer_id)?;
    if points < setting(tx, "loyalty.min_redeem_points")? {
        return Err(AppError::validation("points", "points_below_minimum"));
    }
    let balance = points_balance(tx, customer_id)?;
    if points > balance {
        return Err(AppError::rejected(
            json!({ "balance": balance }),
            "points_insufficient",
        ));
    }
    let value = Money(points)
        .checked_mul_i64(setting(tx, "loyalty.point_value")?)
        .ok_or_else(|| AppError::validation("points", "amount_invalid"))?;
    let voucher = gift_vouchers::issue(
        tx,
        ctx,
        VoucherKind::Loyalty,
        value,
        Some(customer_id),
        None,
        "points",
    )?;
    tx.execute(
        "INSERT INTO loyalty_ledger (id, customer_id, kind, points, voucher_id, business_date, created_at, user_id,
                                     terminal_id)
         VALUES (?1, ?2, 'redeem', ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            new_id(),
            customer_id,
            -points,
            voucher.id,
            ctx.business_day(),
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "loyalty.redeemed",
            "customer",
            Some(customer_id),
            json!({ "points": points, "voucher": voucher.code, "value": value }),
        ),
    )?;
    Ok(voucher)
}

/// Adds or removes points by hand (a paper card carried over, a correction) with its reason:
/// `customers.manage`, no expiry, never below zero. One `loyalty.adjusted` row.
pub fn adjust(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    customer_id: &str,
    points: i64,
    note: &str,
) -> Result<Account, AppError> {
    packs::require(tx, Module::Loyalty)?;
    if !ctx.allows(Permission::CustomersManage) {
        return Err(AppError::Permission {
            permission: Permission::CustomersManage.as_str().to_owned(),
        });
    }
    require_enrolled(tx, customer_id)?;
    if points == 0 {
        return Err(AppError::validation("points", "value_invalid"));
    }
    let note = crate::catalog::clean_opt(Some(note), "note", 200)?
        .ok_or_else(|| AppError::validation("note", "required"))?;
    let balance = points_balance(tx, customer_id)?;
    if balance.saturating_add(points) < 0 {
        return Err(AppError::rejected(
            json!({ "balance": balance }),
            "points_insufficient",
        ));
    }
    tx.execute(
        "INSERT INTO loyalty_ledger (id, customer_id, kind, points, note, business_date, created_at, user_id, terminal_id)
         VALUES (?1, ?2, 'adjust', ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            new_id(),
            customer_id,
            points,
            note,
            ctx.business_day(),
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "loyalty.adjusted",
            "customer",
            Some(customer_id),
            json!({ "points": points, "note": note, "balance": balance.saturating_add(points) }),
        ),
    )?;
    account(tx, customer_id, &ctx.business_day())
}

/// Writes off the earned points whose validity ended before `today` and that the points gone
/// since (redeemed, expired, taken back, adjusted down) did not consume, oldest first: one
/// `expire` row per customer, one `loyalty.expired` row with the count. Returns how many
/// customers lost points.
pub fn expire_due(tx: &Transaction<'_>, actor: &Actor, today: &str) -> Result<u32, AppError> {
    let due: Vec<(String, i64, i64, i64)> = tx
        .prepare(
            "SELECT customer_id,
                    SUM(CASE WHEN kind = 'earn' AND expires_on < ?1 THEN points ELSE 0 END),
                    -SUM(CASE WHEN kind IN ('redeem', 'expire', 'reversal') OR (kind = 'adjust' AND points < 0)
                              THEN points ELSE 0 END),
                    SUM(points)
             FROM loyalty_ledger
             GROUP BY customer_id
             HAVING SUM(CASE WHEN kind = 'earn' AND expires_on < ?1 THEN points ELSE 0 END) > 0
             ORDER BY customer_id",
        )?
        .query_map([today], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let user_id = actor.user_id.clone().unwrap_or_else(|| "system".into());
    let terminal_id = actor.terminal_id.clone().unwrap_or_else(|| "system".into());
    let now = crate::time::now_ms();
    let mut expired = Vec::new();
    for (customer_id, earned, used, balance) in due {
        let points = earned.saturating_sub(used).min(balance);
        if points <= 0 {
            continue;
        }
        tx.execute(
            "INSERT INTO loyalty_ledger (id, customer_id, kind, points, business_date, created_at, user_id, terminal_id)
             VALUES (?1, ?2, 'expire', ?3, ?4, ?5, ?6, ?7)",
            params![new_id(), customer_id, -points, today, now, user_id, terminal_id],
        )?;
        expired.push(json!({ "customerId": customer_id, "points": points }));
    }
    let n = u32::try_from(expired.len()).unwrap_or(u32::MAX);
    if n > 0 {
        audit::append(
            tx,
            &actor.event(
                "loyalty.expired",
                "customer",
                None,
                json!({ "count": n, "day": today, "customers": expired }),
            ),
        )?;
    }
    Ok(n)
}
