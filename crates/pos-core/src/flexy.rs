//! Flexy top-ups (VRT-002, KSK, TEL): a top-up is sold as an open-price line of its operator's
//! service product, the customer's number in the line's note; the operator's float (the credit the
//! shop bought from its distributor) goes down in the same transaction and may never go negative;
//! the shop's commission is the operator's rate on what it sold. A top-up cannot be returned (the
//! credit is spent on the network). USSD sending and scratch-card PIN logs are V1.
//!
//! LAN: the float is a balance, so its writes belong to the host — `purchase` is one
//! function the app registers as a host service; `record_sale` runs inside the sale on the
//! main PC, and a till's top-ups are drawn there once its ticket reached it (Task 31).

use crate::audit::{self, Actor};
use crate::cash::{self, DrawerShare, MovementKind};
use crate::datapack::Pack;
use crate::documents::Document;
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::returns::ReturnInput;
use crate::sales::CartLine;
use crate::sqlenum::sql_text_enum;
use crate::till::TillContext;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

/// One entry of the `flexy.operators` setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OperatorSetting {
    pub code: String,
    pub product_id: String,
    pub commission_bp: i32,
}

/// A mobile operator of the data pack with the shop's setting and its float.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FlexyOperator {
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub prefixes: Vec<String>,
    /// The top-up product; `None` until the template (or the owner) sets it.
    pub product_id: Option<String>,
    pub commission_bp: i32,
    /// Credit left (the sum of the float ledger).
    pub balance: Money,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FloatKind {
    Purchase,
    Sale,
    Reversal,
    Adjustment,
}

impl FloatKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            FloatKind::Purchase => "purchase",
            FloatKind::Sale => "sale",
            FloatKind::Reversal => "reversal",
            FloatKind::Adjustment => "adjustment",
        }
    }

    pub fn parse(s: &str) -> Option<FloatKind> {
        [
            FloatKind::Purchase,
            FloatKind::Sale,
            FloatKind::Reversal,
            FloatKind::Adjustment,
        ]
        .into_iter()
        .find(|k| k.as_str() == s)
    }
}

sql_text_enum!(FloatKind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FloatPurchase {
    pub operator: String,
    /// Credit added to the float (face value).
    pub amount: Money,
    /// What the shop paid the distributor.
    pub cost: Money,
    /// Paid from this till's drawer (a `paid_out`), else paid outside the till.
    pub from_till: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FloatEntry {
    pub id: String,
    pub operator: String,
    pub kind: FloatKind,
    /// Signed: credit in (purchase) or out (sale).
    pub amount: Money,
    pub cost: Money,
    pub document_id: Option<String>,
    /// The last 4 digits of the number topped up (a sale).
    pub phone_tail: Option<String>,
    pub business_date: String,
    pub created_at: i64,
}

const ENTRY_SELECT: &str =
    "SELECT id, operator, kind, amount, cost, document_id, phone_tail, business_date, created_at
     FROM flexy_float";

fn map_entry(r: &Row<'_>) -> rusqlite::Result<FloatEntry> {
    Ok(FloatEntry {
        id: r.get(0)?,
        operator: r.get(1)?,
        kind: r.get(2)?,
        amount: r.get(3)?,
        cost: r.get(4)?,
        document_id: r.get(5)?,
        phone_tail: r.get(6)?,
        business_date: r.get(7)?,
        created_at: r.get(8)?,
    })
}

fn settings(conn: &Connection) -> Result<Vec<OperatorSetting>, AppError> {
    Ok(serde_json::from_value(crate::settings::get_value(
        conn,
        "flexy.operators",
    )?)?)
}

fn balance(conn: &Connection, operator: &str) -> Result<Money, AppError> {
    Ok(conn
        .prepare_cached("SELECT COALESCE(SUM(amount), 0) FROM flexy_float WHERE operator = ?1")?
        .query_row([operator], |r| r.get(0))?)
}

/// The data pack's operators with the shop's settings and floats.
pub fn operators(conn: &Connection, pack: &Pack) -> Result<Vec<FlexyOperator>, AppError> {
    let set = settings(conn)?;
    pack.mobile_operators
        .iter()
        .map(|o| {
            let s = set.iter().find(|s| s.code == o.code);
            Ok(FlexyOperator {
                code: o.code.clone(),
                name_ar: o.name_ar.clone(),
                name_fr: o.name_fr.clone(),
                name_en: o.name_en.clone(),
                prefixes: o.prefixes.clone(),
                product_id: s.map(|s| s.product_id.clone()),
                commission_bp: s.map_or(0, |s| s.commission_bp),
                balance: balance(conn, &o.code)?,
            })
        })
        .collect()
}

/// The national number's digits (`+213 770…` → `0770…`).
fn national(phone: &str) -> String {
    let digits: String = crate::party::western_digits(phone)
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    match digits.strip_prefix("213") {
        Some(rest) if rest.len() == 9 => format!("0{rest}"),
        _ => digits,
    }
}

/// The operator a number most likely belongs to (by its prefix); the cashier can change it.
pub fn guess(pack: &Pack, phone: &str) -> Option<String> {
    let n = national(phone);
    pack.mobile_operators
        .iter()
        .find(|o| o.prefixes.iter().any(|p| n.starts_with(p.as_str())))
        .map(|o| o.code.clone())
}

/// `0661234567` → `0661 23 45 67`.
fn formatted(n: &str) -> String {
    if n.len() == 10 {
        format!("{} {} {} {}", &n[0..4], &n[4..6], &n[6..8], &n[8..10])
    } else {
        n.to_owned()
    }
}

/// The cart line of a top-up of `amount` to `phone` on `operator`.
pub fn line(
    conn: &Connection,
    pack: &Pack,
    operator: &str,
    phone: &str,
    amount: Money,
) -> Result<CartLine, AppError> {
    packs::require(conn, Module::Flexy)?;
    let op = operators(conn, pack)?
        .into_iter()
        .find(|o| o.code == operator)
        .ok_or_else(|| AppError::validation("operator", "flexy_operator_unknown"))?;
    let product_id = op
        .product_id
        .ok_or_else(|| AppError::validation("operator", "flexy_operator_unknown"))?;
    let n = national(phone);
    if n.len() != 10 || !crate::party::is_valid_phone(&n) {
        return Err(AppError::validation("phone", "phone_invalid"));
    }
    if !amount.is_positive() {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    let mut line = CartLine::product(&product_id, Qty(1_000));
    line.unit_price = Some(amount);
    line.note = Some(formatted(&n));
    Ok(line)
}

/// Buys float from the distributor: the credit comes in, and the cash goes out of the drawer
/// when it is paid from the till. One `flexy.float_purchased` row.
pub fn purchase(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    pack: &Pack,
    input: FloatPurchase,
) -> Result<FloatEntry, AppError> {
    packs::require(tx, Module::Flexy)?;
    if !pack
        .mobile_operators
        .iter()
        .any(|o| o.code == input.operator)
    {
        return Err(AppError::validation("operator", "flexy_operator_unknown"));
    }
    if !input.amount.is_positive() {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    if input.cost.is_negative() {
        return Err(AppError::validation("cost", "amount_invalid"));
    }
    let note = crate::catalog::clean_opt(input.note.as_deref(), "note", 200)?;
    // Paid from the drawer of this till, or of the calling till that records it (Task 20 review).
    let movement = match purchase_share(&input)? {
        Some(share) => {
            let session = cash::drawer(tx, ctx)?;
            cash::book(tx, ctx, &session.id, &share)?
        }
        None => None,
    };
    let id = new_id();
    tx.execute(
        "INSERT INTO flexy_float (id, operator, kind, amount, cost, cash_movement_id, business_date, note,
                                  created_at, user_id, terminal_id)
         VALUES (?1, ?2, 'purchase', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            input.operator,
            input.amount,
            input.cost,
            movement,
            ctx.business_day(),
            note,
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "flexy.float_purchased",
            "flexy_float",
            Some(&id),
            json!({
                "operator": input.operator,
                "amount": input.amount,
                "cost": input.cost,
                "fromTill": input.from_till,
            }),
        ),
    )?;
    Ok(tx
        .prepare_cached(&format!("{ENTRY_SELECT} WHERE id = ?1"))?
        .query_row([&id], map_entry)?)
}

/// The drawer movement of float paid from the till: the cost out of the drawer, with the
/// purchase's note. `None` when it is paid outside the till. A host service for a till returns
/// the entry and the till records this share in its own drawer (Task 20 review).
pub fn purchase_share(input: &FloatPurchase) -> Result<Option<DrawerShare>, AppError> {
    if !(input.from_till && input.cost.is_positive()) {
        return Ok(None);
    }
    let out = input
        .cost
        .checked_neg()
        .ok_or_else(|| AppError::validation("cost", "amount_invalid"))?;
    Ok(Some(DrawerShare {
        kind: MovementKind::PaidOut,
        amount: out,
        ref_type: "flexy_float".into(),
        ref_id: input.operator.clone(),
        note: crate::catalog::clean_opt(input.note.as_deref(), "note", 200)?,
        category: None,
    }))
}

/// A top-up line of a validated ticket: its operator, the credit it sends and the last four
/// digits of the number it went to.
struct TopUp<'a> {
    operator: String,
    line: &'a crate::documents::DocumentLine,
    face: Money,
    tail: Option<String>,
}

/// The operator and the credit sent of a ticket line that is a top-up. The credit sent is the
/// typed price: a discount comes out of the shop's margin, not the operator's float.
fn top_up_of<'s>(
    set: &'s [OperatorSetting],
    line: &crate::documents::DocumentLine,
) -> Result<Option<(&'s str, Money)>, AppError> {
    let Some(operator) = line
        .product_id
        .as_ref()
        .and_then(|pid| set.iter().find(|s| &s.product_id == pid))
    else {
        return Ok(None);
    };
    let face = line
        .unit_price
        .mul_qty(line.qty)
        .ok_or_else(|| AppError::validation("lines", "amount_invalid"))?;
    Ok(Some((operator.code.as_str(), face)))
}

/// The top-ups of a validated ticket, each checked against what its operator's float still holds
/// after the ticket's earlier top-ups and those of the `waiting` tickets (a till's tickets the
/// main PC has not drawn yet, Task 31 review) (`flexy_float_insufficient` when the credit is
/// short).
fn top_ups<'a>(
    conn: &Connection,
    document: &'a Document,
    main_line_ids: &[String],
    waiting: &[Document],
) -> Result<Vec<TopUp<'a>>, AppError> {
    let set = settings(conn)?;
    let mut out: Vec<TopUp<'a>> = Vec::new();
    if set.is_empty() {
        return Ok(out);
    }
    let mut claimed: Vec<(&str, Money)> = Vec::new();
    for line in waiting.iter().flat_map(|t| &t.lines) {
        claimed.extend(top_up_of(&set, line)?);
    }
    for id in main_line_ids {
        let Some(line) = document.lines.iter().find(|l| &l.id == id) else {
            continue;
        };
        let Some((operator, face)) = top_up_of(&set, line)? else {
            continue;
        };
        let earlier = Money::checked_sum(
            claimed
                .iter()
                .filter(|(op, _)| *op == operator)
                .map(|(_, face)| *face)
                .chain(
                    out.iter()
                        .filter(|t| t.operator == operator)
                        .map(|t| t.face),
                ),
        )
        .ok_or_else(|| AppError::validation("lines", "amount_invalid"))?;
        let left = balance(conn, operator)?
            .checked_sub(earlier)
            .ok_or_else(|| AppError::validation("lines", "amount_invalid"))?;
        if left < face {
            return Err(AppError::rejected(
                json!({ "operator": operator }),
                "flexy_float_insufficient",
            ));
        }
        let tail: Option<String> = line.note.as_deref().and_then(|n| {
            let d: String = n.chars().filter(char::is_ascii_digit).collect();
            (d.len() >= 4).then(|| d[d.len() - 4..].to_owned())
        });
        out.push(TopUp {
            operator: operator.to_owned(),
            line,
            face,
            tail,
        });
    }
    Ok(out)
}

/// The sold top-ups of a validated ticket lower their operator's float (called by
/// `sales::extensions::after_sale`, and on the main PC for a till's ticket);
/// `flexy_float_insufficient` when the credit is short.
pub(crate) fn record_sale(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    document: &Document,
    main_line_ids: &[String],
) -> Result<(), AppError> {
    for t in top_ups(tx, document, main_line_ids, &[])? {
        let amount = t
            .face
            .checked_neg()
            .ok_or_else(|| AppError::validation("lines", "amount_invalid"))?;
        tx.prepare_cached(
            "INSERT INTO flexy_float (id, operator, kind, amount, document_id, line_id, phone_tail, business_date,
                                      created_at, user_id, terminal_id)
             VALUES (?1, ?2, 'sale', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?
        .execute(params![
            new_id(),
            t.operator,
            amount,
            document.id,
            t.line.id,
            t.tail,
            ctx.business_day(),
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ])?;
    }
    Ok(())
}

/// A till's ticket (the float is the main PC's): its top-ups checked against the float
/// of the till's copy, less what the till's `waiting` tickets drew that the main PC has not drawn
/// yet, and left to the main PC to draw. Whether the ticket has any.
pub(crate) fn check_sale(
    conn: &Connection,
    document: &Document,
    main_line_ids: &[String],
    waiting: &[Document],
) -> Result<bool, AppError> {
    Ok(!top_ups(conn, document, main_line_ids, waiting)?.is_empty())
}

/// Whether `product_id` is an operator's top-up product.
pub(crate) fn is_top_up(conn: &Connection, product_id: &str) -> Result<bool, AppError> {
    Ok(settings(conn)?.iter().any(|s| s.product_id == product_id))
}

/// A top-up is spent on the network: its line is never returned (`flexy_not_returnable`).
pub(crate) fn refuse_return(conn: &Connection, input: &ReturnInput) -> Result<(), AppError> {
    let set = settings(conn)?;
    if set.is_empty() {
        return Ok(());
    }
    for (i, line) in input.lines.iter().enumerate() {
        let product: Option<String> = match (&line.origin_line_id, &line.product_id) {
            (Some(origin), _) => conn
                .prepare_cached("SELECT product_id FROM document_lines WHERE id = ?1")?
                .query_row([origin], |r| r.get(0))
                .optional()?
                .flatten(),
            (None, p) => p.clone(),
        };
        if product.is_some_and(|p| set.iter().any(|s| s.product_id == p)) {
            return Err(AppError::validation(
                &format!("lines.{i}"),
                "flexy_not_returnable",
            ));
        }
    }
    Ok(())
}

/// The float left per operator of the data pack.
pub fn balances(conn: &Connection, pack: &Pack) -> Result<Vec<(String, Money)>, AppError> {
    pack.mobile_operators
        .iter()
        .map(|o| Ok((o.code.clone(), balance(conn, &o.code)?)))
        .collect()
}

/// The float ledger of the business days `from`..=`to`, of one operator or all.
pub fn entries(
    conn: &Connection,
    operator: Option<&str>,
    from: &str,
    to: &str,
) -> Result<Vec<FloatEntry>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{ENTRY_SELECT} WHERE (?1 IS NULL OR operator = ?1) AND business_date BETWEEN ?2 AND ?3
         ORDER BY created_at, id"
    ))?;
    let rows = stmt
        .query_map(params![operator, from, to], map_entry)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The template's top-up products: one open-price service per operator of the pack, and the
/// `flexy.operators` setting pointing at them (commission 0 until the owner types it). The
/// number of operators set up; one `flexy.operators_seeded` row when any.
pub fn seed(tx: &Transaction<'_>, actor: &Actor, pack: &Pack) -> Result<u32, AppError> {
    let mut set = settings(tx)?;
    let mut created = 0;
    for op in &pack.mobile_operators {
        if set.iter().any(|s| s.code == op.code) {
            continue;
        }
        let code = format!("SVC-FLEXY-{}", op.code.to_uppercase());
        let (ar, fr, en) = (
            format!("فليكسي {}", op.name_ar),
            format!("Flexy {}", op.name_fr),
            format!("Top-up {}", op.name_en),
        );
        let product_id = match crate::packs::seeds::service_product_named(
            tx,
            actor,
            &code,
            (&ar, &fr, &en),
            crate::catalog::UNIT_PIECE_ID,
            "EXO",
            Money::ZERO,
            true,
        )? {
            Some(id) => id,
            None => tx.query_row(
                "SELECT id FROM products WHERE reference = ?1",
                [&code],
                |r| r.get(0),
            )?,
        };
        set.push(OperatorSetting {
            code: op.code.clone(),
            product_id,
            commission_bp: 0,
        });
        created += 1;
    }
    if created > 0 {
        crate::settings::set_quiet(tx, "flexy.operators", serde_json::to_value(&set)?)?;
        audit::append(
            tx,
            &actor.event(
                "flexy.operators_seeded",
                "setting",
                Some("flexy.operators"),
                json!({ "operators": created }),
            ),
        )?;
    }
    Ok(created)
}
