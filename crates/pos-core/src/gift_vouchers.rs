//! Gift, exchange and loyalty vouchers (LOY-001, RET-007, PAY-010/011 lite): a voucher is a
//! printed code `BA2026-000001` with an EAN-13 `298yy` + sequence, an amount, a validity and an
//! append-only ledger (issue, redeem, expire). A gift voucher is sold (the droit de timbre on
//! its cash, the cash in the drawer as a paid-in: it is not turnover, the sale it pays is);
//! an exchange voucher is a return's refund; a loyalty voucher is points turned into money.
//! All three are spent by the `voucher` tender through `vouchers::book`, never beyond their
//! balance (the ledger's trigger) and never after their expiry.
//!
//! LAN: a voucher's balance is a ledger sum, so `sell` and `expire_due` are single
//! functions the app registers as host services; `issue` runs inside a return or a points
//! redemption, `spend` inside the sale that tenders the voucher.

use crate::audit::{self, Actor};
use crate::cash::{self, DrawerShare, MovementKind};
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::Money;
use crate::packs::{self, Module};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sqlenum::sql_text_enum;
use crate::till::TillContext;
use chrono::{Months, NaiveDate};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum VoucherKind {
    Gift,
    Exchange,
    Loyalty,
}

impl VoucherKind {
    pub const ALL: [VoucherKind; 3] = [
        VoucherKind::Gift,
        VoucherKind::Exchange,
        VoucherKind::Loyalty,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            VoucherKind::Gift => "gift",
            VoucherKind::Exchange => "exchange",
            VoucherKind::Loyalty => "loyalty",
        }
    }

    pub fn parse(s: &str) -> Option<VoucherKind> {
        VoucherKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(VoucherKind);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum VoucherStatus {
    Active,
    Spent,
    Expired,
}

impl VoucherStatus {
    pub const ALL: [VoucherStatus; 3] = [
        VoucherStatus::Active,
        VoucherStatus::Spent,
        VoucherStatus::Expired,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            VoucherStatus::Active => "active",
            VoucherStatus::Spent => "spent",
            VoucherStatus::Expired => "expired",
        }
    }

    pub fn parse(s: &str) -> Option<VoucherStatus> {
        VoucherStatus::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(VoucherStatus);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Voucher {
    pub id: String,
    /// `BA2026-000001`, printed on the voucher.
    pub code: String,
    /// The EAN-13 printed under the code (`298` + year + sequence).
    pub barcode: String,
    pub kind: VoucherKind,
    pub initial_amount: Money,
    /// What is left: the sum of the voucher's ledger.
    pub balance: Money,
    pub status: VoucherStatus,
    pub issued_on: String,
    pub expires_on: String,
    /// The buyer of a gift voucher or the owner of a loyalty voucher (the report's, never checked
    /// at the till: a voucher is a bearer voucher).
    pub customer_id: Option<String>,
    pub buyer_name: Option<String>,
    /// The return note an exchange voucher refunds.
    pub source_document_id: Option<String>,
    /// The droit de timbre on the money received for a gift voucher sold (its `issue` row), printed
    /// on its slip; zero when paid by card and for exchange and loyalty vouchers.
    pub stamp_duty: Money,
    /// The part of `stamp_duty` the buyer paid on top of the amount.
    pub stamp_charged: Money,
}

#[cfg(any(test, feature = "testkit"))]
impl Voucher {
    /// A fixed 5 000 DA gift voucher sold for cash (50 DA of duty charged), for the slip tests of
    /// `pos-docs`.
    pub fn sample() -> Voucher {
        Voucher {
            id: "00000000-0000-7000-8000-0000000000b1".into(),
            code: "BA2026-000001".into(),
            barcode: "2982600000018".into(),
            kind: VoucherKind::Gift,
            initial_amount: Money(500_000),
            balance: Money(500_000),
            status: VoucherStatus::Active,
            issued_on: "2026-09-28".into(),
            expires_on: "2027-09-28".into(),
            customer_id: None,
            buyer_name: Some("Karim".into()),
            source_document_id: None,
            stamp_duty: Money(5_000),
            stamp_charged: Money(5_000),
        }
    }
}

/// A gift voucher sold at the counter (`Serialize`: a till sends it to the main PC's host service).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SellInput {
    pub amount: Money,
    /// Cash, card CIB, Edahabia, BaridiMob, cheque or transfer (never kridi nor a voucher).
    pub method: PaymentMethod,
    pub reference: Option<String>,
    pub cheque_bank: Option<String>,
    pub cheque_date: Option<String>,
    pub customer_id: Option<String>,
    pub buyer_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaleQuote {
    pub amount: Money,
    pub stamp_duty: Money,
    pub stamp_charged: Money,
    /// What the buyer hands over: the amount and the duty charged.
    pub to_receive: Money,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct VoucherFilter {
    pub kind: Option<VoucherKind>,
    pub status: Option<VoucherStatus>,
    /// Part of the code, the scanned barcode or part of the buyer's name.
    pub text: String,
}

fn invalid_code() -> AppError {
    AppError::validation("code", "value_invalid")
}

/// `298` + the two last digits of the year + the sequence on seven digits + the EAN-13 check
/// digit (`BA2026-000001` → `298260000001` + check).
fn barcode_of(number: &str) -> Result<String, AppError> {
    let year: i32 = number
        .get(2..6)
        .and_then(|y| y.parse().ok())
        .ok_or_else(invalid_code)?;
    let seq: i64 = number
        .get(7..)
        .and_then(|s| s.parse().ok())
        .ok_or_else(invalid_code)?;
    let body = format!("298{:02}{seq:07}", year.rem_euclid(100));
    if body.len() != 12 {
        return Err(invalid_code());
    }
    let check = crate::labels::build::gtin_check_digit(&body).ok_or_else(invalid_code)?;
    Ok(format!("{body}{check}"))
}

/// The voucher number behind a scanned `298…` barcode (`None` for any other code).
pub fn code_of_barcode(barcode: &str) -> Option<String> {
    let b = barcode.trim();
    if b.len() != 13 || !b.starts_with("298") || !b.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let check = crate::labels::build::gtin_check_digit(b.get(..12)?)?;
    if b.as_bytes().get(12).map(|d| d - b'0') != Some(check) {
        return None;
    }
    let year: i32 = b.get(3..5)?.parse().ok()?;
    let seq: i64 = b.get(5..12)?.parse().ok()?;
    Some(crate::numbering::format_custom("BA", 2000 + year, seq))
}

fn validity(conn: &Connection, from: NaiveDate) -> Result<String, AppError> {
    let months: i64 = crate::settings::get(conn, "vouchers.validity_months")?;
    let until = u32::try_from(months)
        .ok()
        .and_then(|m| from.checked_add_months(Months::new(m)))
        .ok_or_else(|| AppError::validation("expiresOn", "date_invalid"))?;
    Ok(until.format("%Y-%m-%d").to_string())
}

/// The checked tender of a gift voucher sold: M2's versement rules (no kridi nor voucher,
/// cheque details, transfer reference).
fn tender(input: &SellInput) -> Result<PaymentInput, AppError> {
    crate::customers::credit::clean_versement(&crate::customers::credit::CollectInput {
        customer_id: String::new(),
        amount: input.amount,
        method: input.method,
        reference: input.reference.clone(),
        cheque_bank: input.cheque_bank.clone(),
        cheque_date: input.cheque_date.clone(),
        allocate_to: Vec::new(),
        note: None,
    })
}

/// The figures of a gift voucher before it is sold: the droit de timbre on its cash through M2's
/// `versement_duty` (`tax::timbre::for_cash_amount`), the part charged and what to receive.
pub fn quote_sale(
    conn: &Connection,
    ctx: &TillContext,
    input: &SellInput,
) -> Result<SaleQuote, AppError> {
    packs::require(conn, Module::GiftVouchers)?;
    let t = tender(input)?;
    let (stamp_duty, stamp_charged, _) = crate::customers::credit::versement_duty(conn, ctx, &t)?;
    Ok(SaleQuote {
        amount: t.amount,
        stamp_duty,
        stamp_charged,
        to_receive: t
            .amount
            .checked_add(stamp_charged)
            .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?,
    })
}

struct NewVoucher<'a> {
    kind: VoucherKind,
    amount: Money,
    customer_id: Option<&'a str>,
    buyer_name: Option<String>,
    source_document_id: Option<&'a str>,
}

/// Numbers and stores a voucher; its `issue` ledger row is the caller's.
fn insert_voucher(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    v: NewVoucher<'_>,
) -> Result<(String, String), AppError> {
    if !v.amount.is_positive() {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    let number =
        crate::numbering::assign_custom(tx, "gift_voucher", "BA", ctx.business_date())?.number;
    let barcode = barcode_of(&number)?;
    let id = new_id();
    tx.execute(
        "INSERT INTO gift_vouchers (id, code, barcode, kind, initial_amount, issued_on, expires_on, customer_id,
                                    buyer_name, source_document_id, created_at, user_id, terminal_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            id,
            number,
            barcode,
            v.kind,
            v.amount,
            ctx.business_day(),
            validity(tx, ctx.business_date())?,
            v.customer_id,
            v.buyer_name,
            v.source_document_id,
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    Ok((id, number))
}

/// Sells a gift voucher: its `BA…` number and barcode, the droit de timbre on its cash, the cash
/// in the drawer as a `paid_in` movement (not turnover: the sale the voucher pays is). One
/// `vouchers.sold` row. Sold on the main PC for a till (a host service), its cash is
/// booked on the till's open session and the till records the movement ([`drawer_share`]).
pub fn sell(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: SellInput,
) -> Result<Voucher, AppError> {
    let q = quote_sale(tx, ctx, &input)?;
    let t = tender(&input)?;
    if let Some(c) = &input.customer_id {
        crate::customers::selectable(tx, c)?;
    }
    // The drawer that takes the cash: this till's, or the calling till's.
    let session = if t.method.is_cash() {
        Some(cash::drawer(tx, ctx)?)
    } else {
        cash::drawer_if_open(tx, ctx)?
    };
    let buyer = crate::catalog::clean_opt(input.buyer_name.as_deref(), "buyerName", 120)?;
    let (id, number) = insert_voucher(
        tx,
        ctx,
        NewVoucher {
            kind: VoucherKind::Gift,
            amount: t.amount,
            customer_id: input.customer_id.as_deref(),
            buyer_name: buyer,
            source_document_id: None,
        },
    )?;
    tx.execute(
        "INSERT INTO gift_voucher_ledger (id, voucher_id, kind, amount, method, reference, stamp_duty, stamp_charged,
                                          cash_session_id, business_date, created_at, user_id, terminal_id)
         VALUES (?1, ?2, 'issue', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            new_id(),
            id,
            t.amount,
            t.method,
            t.reference,
            q.stamp_duty,
            q.stamp_charged,
            session.as_ref().map(|s| s.id.clone()),
            ctx.business_day(),
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    let voucher = get(tx, &number)?;
    if let (Some(share), Some(s)) = (drawer_share(&voucher, t.method), &session) {
        if share.amount != q.to_receive {
            return Err(AppError::Internal(
                "a voucher's drawer share differs from its quote".into(),
            ));
        }
        cash::book(tx, ctx, &s.id, &share)?;
    }
    audit::append(
        tx,
        &ctx.actor().event(
            "vouchers.sold",
            "gift_voucher",
            Some(&id),
            json!({
                "code": number,
                "amount": t.amount,
                "method": t.method,
                "stampDuty": q.stamp_duty,
                "stampCharged": q.stamp_charged,
            }),
        ),
    )?;
    Ok(voucher)
}

/// The drawer movement of a gift voucher sold for cash: what the buyer handed over (its amount
/// and the duty charged), a `paid_in` naming the voucher. `None` for any other tender or kind. A
/// host service for a till answers the voucher and the till records this share in its own drawer.
pub fn drawer_share(voucher: &Voucher, method: PaymentMethod) -> Option<DrawerShare> {
    if !method.is_cash() || voucher.kind != VoucherKind::Gift {
        return None;
    }
    Some(DrawerShare {
        kind: MovementKind::PaidIn,
        amount: voucher.initial_amount.checked_add(voucher.stamp_charged)?,
        ref_type: "gift_voucher".into(),
        ref_id: voucher.id.clone(),
        note: Some(voucher.code.clone()),
        category: None,
    })
}

/// Exchange and loyalty vouchers: issued by a return or by points, no money received now
/// (`method` is the ledger's `return` or `points`). Journaled by the caller's own row.
pub(crate) fn issue(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    kind: VoucherKind,
    amount: Money,
    customer_id: Option<&str>,
    source_document_id: Option<&str>,
    method: &str,
) -> Result<Voucher, AppError> {
    let (id, number) = insert_voucher(
        tx,
        ctx,
        NewVoucher {
            kind,
            amount,
            customer_id,
            buyer_name: None,
            source_document_id,
        },
    )?;
    tx.execute(
        "INSERT INTO gift_voucher_ledger (id, voucher_id, kind, amount, method, document_id, business_date, created_at,
                                          user_id, terminal_id)
         VALUES (?1, ?2, 'issue', ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            new_id(),
            id,
            amount,
            method,
            source_document_id,
            ctx.business_day(),
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    get(tx, &number)
}

/// What [`spend`] refuses, without spending: an expired voucher, a non-positive amount, more than
/// its balance. The voucher.
pub(crate) fn check_spend(
    conn: &Connection,
    ctx: &TillContext,
    code: &str,
    amount: Money,
) -> Result<Voucher, AppError> {
    let v = get(conn, code)?;
    if v.status == VoucherStatus::Expired || v.expires_on < ctx.business_day() {
        return Err(AppError::rejected(
            json!({ "code": v.code, "expiresOn": v.expires_on }),
            "voucher_expired",
        ));
    }
    if !amount.is_positive() {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    if amount > v.balance {
        return Err(AppError::rejected(
            json!({ "code": v.code, "balance": v.balance }),
            "voucher_balance_insufficient",
        ));
    }
    Ok(v)
}

/// Spends `amount` of the voucher on `document_id` (a ticket's `voucher` tender): never after
/// its expiry, never beyond its balance. Returns the balance left.
pub(crate) fn spend(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    code: &str,
    amount: Money,
    document_id: &str,
) -> Result<Money, AppError> {
    let v = check_spend(tx, ctx, code, amount)?;
    let out = amount
        .checked_neg()
        .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?;
    tx.execute(
        "INSERT INTO gift_voucher_ledger (id, voucher_id, kind, amount, document_id, business_date, created_at, user_id,
                                          terminal_id)
         VALUES (?1, ?2, 'redeem', ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            new_id(),
            v.id,
            out,
            document_id,
            ctx.business_day(),
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    v.balance
        .checked_sub(amount)
        .ok_or_else(|| AppError::validation("amount", "amount_invalid"))
}

/// Every voucher with its balance, whether an `expire` row closed it, and the duty of its `issue`
/// row.
const SELECT: &str = "SELECT * FROM (
        SELECT v.id, v.code, v.barcode, v.kind, v.initial_amount,
               (SELECT COALESCE(SUM(l.amount), 0) FROM gift_voucher_ledger l WHERE l.voucher_id = v.id) AS balance,
               EXISTS (SELECT 1 FROM gift_voucher_ledger l WHERE l.voucher_id = v.id AND l.kind = 'expire') AS expired,
               v.issued_on, v.expires_on, v.customer_id, v.buyer_name, v.source_document_id, v.created_at,
               (SELECT COALESCE(SUM(l.stamp_duty), 0) FROM gift_voucher_ledger l
                WHERE l.voucher_id = v.id AND l.kind = 'issue') AS stamp_duty,
               (SELECT COALESCE(SUM(l.stamp_charged), 0) FROM gift_voucher_ledger l
                WHERE l.voucher_id = v.id AND l.kind = 'issue') AS stamp_charged
        FROM gift_vouchers v) x";

/// The status as of `today`: a voucher past its validity with money left reads expired before
/// `expire_due` writes it off (`today` empty: only the written-off ones).
const STATUS: &str =
    "CASE WHEN x.expired OR (x.expires_on < :today AND x.balance > 0) THEN 'expired'
                           WHEN x.balance > 0 THEN 'active' ELSE 'spent' END";

fn map(r: &rusqlite::Row<'_>) -> rusqlite::Result<Voucher> {
    Ok(Voucher {
        id: r.get(0)?,
        code: r.get(1)?,
        barcode: r.get(2)?,
        kind: r.get(3)?,
        initial_amount: r.get(4)?,
        balance: r.get(5)?,
        status: r.get(15)?,
        issued_on: r.get(7)?,
        expires_on: r.get(8)?,
        customer_id: r.get(9)?,
        buyer_name: r.get(10)?,
        source_document_id: r.get(11)?,
        stamp_duty: r.get(13)?,
        stamp_charged: r.get(14)?,
    })
}

/// A voucher by its printed number or its scanned barcode.
pub fn get(conn: &Connection, code_or_barcode: &str) -> Result<Voucher, AppError> {
    let code =
        code_of_barcode(code_or_barcode).unwrap_or_else(|| code_or_barcode.trim().to_uppercase());
    conn.prepare_cached(&format!(
        "SELECT x.*, {STATUS} FROM ({SELECT}) x WHERE x.code = :code"
    ))?
    .query_row(rusqlite::named_params! { ":code": code, ":today": "" }, map)
    .optional()?
    .ok_or_else(|| AppError::validation("reference", "voucher_unknown"))
}

/// The vouchers of a kind, a status as of `today` or a text, newest first (500 at most).
pub fn list(
    conn: &Connection,
    filter: &VoucherFilter,
    today: &str,
) -> Result<Vec<Voucher>, AppError> {
    let text = filter.text.trim().to_uppercase();
    let barcode_code = code_of_barcode(&text);
    let pattern = format!("%{}%", text.replace(['%', '_'], ""));
    let mut stmt = conn.prepare(&format!(
        "SELECT * FROM (SELECT x.*, {STATUS} AS status FROM ({SELECT}) x)
         WHERE (:kind IS NULL OR kind = :kind)
           AND (:status IS NULL OR status = :status)
           AND (:text = '' OR code = :barcode_code OR code LIKE :pattern
                OR UPPER(COALESCE(buyer_name, '')) LIKE :pattern)
         ORDER BY created_at DESC, code DESC
         LIMIT 500"
    ))?;
    let rows = stmt
        .query_map(
            rusqlite::named_params! {
                ":today": today,
                ":kind": filter.kind,
                ":status": filter.status,
                ":text": text,
                ":barcode_code": barcode_code,
                ":pattern": pattern,
            },
            map,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Writes off the balance of every voucher whose validity ended before `today` (one `expire`
/// ledger row each; the liability leaves the vouchers report). Returns how many expired.
pub fn expire_due(tx: &Transaction<'_>, actor: &Actor, today: &str) -> Result<u32, AppError> {
    let due: Vec<(String, Money)> = tx
        .prepare(
            "SELECT v.id, (SELECT COALESCE(SUM(l.amount), 0) FROM gift_voucher_ledger l WHERE l.voucher_id = v.id)
             FROM gift_vouchers v
             WHERE v.expires_on < ?1
               AND NOT EXISTS (SELECT 1 FROM gift_voucher_ledger l WHERE l.voucher_id = v.id AND l.kind = 'expire')
             ORDER BY v.created_at, v.rowid",
        )?
        .query_map([today], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let user_id = actor.user_id.clone().unwrap_or_else(|| "system".into());
    let terminal_id = actor.terminal_id.clone().unwrap_or_else(|| "system".into());
    let now = crate::time::now_ms();
    let mut codes = Vec::new();
    for (id, balance) in due.into_iter().filter(|(_, b)| b.is_positive()) {
        let out = balance
            .checked_neg()
            .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?;
        tx.execute(
            "INSERT INTO gift_voucher_ledger (id, voucher_id, kind, amount, business_date, created_at, user_id, terminal_id)
             VALUES (?1, ?2, 'expire', ?3, ?4, ?5, ?6, ?7)",
            params![new_id(), id, out, today, now, user_id, terminal_id],
        )?;
        codes.push(id);
    }
    let n = u32::try_from(codes.len()).unwrap_or(u32::MAX);
    if n > 0 {
        audit::append(
            tx,
            &actor.event(
                "vouchers.expired",
                "gift_voucher",
                None,
                json!({ "count": n, "day": today, "voucherIds": codes }),
            ),
        )?;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_voucher_barcode_reads_back_its_number() {
        let barcode = barcode_of("BA2026-000001").unwrap();
        assert_eq!(barcode.len(), 13);
        assert!(barcode.starts_with("298260000001"));
        assert_eq!(code_of_barcode(&barcode).as_deref(), Some("BA2026-000001"));
        let big = barcode_of("BA2027-1234567").unwrap();
        assert_eq!(code_of_barcode(&big).as_deref(), Some("BA2027-1234567"));
        let mut wrong = barcode.clone();
        wrong.replace_range(12.., if barcode.ends_with('0') { "1" } else { "0" });
        assert_eq!(code_of_barcode(&wrong), None, "a bad check digit");
        assert_eq!(
            code_of_barcode("6130000000001"),
            None,
            "not a voucher barcode"
        );
        assert_eq!(Voucher::sample().barcode, barcode);
    }
}
