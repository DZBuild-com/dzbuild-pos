//! Zakat on trade goods (master spec §28 FIN-007; research 09 §5.6): an indicative statement —
//! stock (at cost or at sale price, the owner's choice), cash in the drawers, bank and other
//! assets, receivables, minus supplier debts and other debts — against the data pack's nisab,
//! at the pack's rate (2.5 %). The Hijri date is Umm al-Qura (±1 day against the moon sighting
//! the ministry announces). The statement says it is not a fatwa; it is saved append-only.
//!
//! LAN: drawn up on the host, which holds the books of every till.

use crate::audit;
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::{Money, Rate};
use crate::packs::{self, Module};
use crate::sqlenum::sql_text_enum;
use crate::till::TillContext;
use chrono::{Datelike, NaiveDate};
use icu_calendar::Date;
use icu_calendar::cal::Hijri;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

/// The longest note of a statement.
const NOTE_MAX: usize = 200;
/// The most statements `list` returns.
const PAGE_MAX: i64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HijriDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

/// The Umm al-Qura date of a Gregorian day.
pub fn hijri(date: NaiveDate) -> Result<HijriDate, AppError> {
    let bad = || AppError::validation("day", "date_invalid");
    let month = u8::try_from(date.month()).map_err(|_| bad())?;
    let day = u8::try_from(date.day()).map_err(|_| bad())?;
    let iso = Date::try_new_iso(date.year(), month, day).map_err(|_| bad())?;
    let h = iso.to_calendar(Hijri::new_umm_al_qura());
    Ok(HijriDate {
        year: h.year().extended_year(),
        month: h.month().ordinal,
        day: h.day_of_month().0,
    })
}

const MONTHS_AR: [&str; 12] = [
    "محرم",
    "صفر",
    "ربيع الأول",
    "ربيع الآخر",
    "جمادى الأولى",
    "جمادى الآخرة",
    "رجب",
    "شعبان",
    "رمضان",
    "شوال",
    "ذو القعدة",
    "ذو الحجة",
];
const MONTHS_LATIN: [&str; 12] = [
    "Muharram",
    "Safar",
    "Rabi' al-Awwal",
    "Rabi' al-Thani",
    "Joumada al-Oula",
    "Joumada al-Akhira",
    "Rajab",
    "Cha'bane",
    "Ramadan",
    "Chawwal",
    "Dhou al-Qi'da",
    "Dhou al-Hijja",
];

/// "17 Rabi' al-Thani 1448" / "17 ربيع الآخر 1448" / "17 Rabi' al-Thani 1448 AH".
pub fn hijri_label(h: HijriDate, lang: Lang) -> String {
    let i = usize::from(h.month.clamp(1, 12)) - 1;
    match lang {
        Lang::Ar => format!("{} {} {}", h.day, MONTHS_AR[i], h.year),
        Lang::Fr => format!("{} {} {}", h.day, MONTHS_LATIN[i], h.year),
        Lang::En => format!("{} {} {} AH", h.day, MONTHS_LATIN[i], h.year),
    }
}

/// How the stock is valued in the base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Valuation {
    Cost,
    SalePrice,
}

impl Valuation {
    pub const ALL: [Valuation; 2] = [Valuation::Cost, Valuation::SalePrice];

    pub const fn as_str(self) -> &'static str {
        match self {
            Valuation::Cost => "cost",
            Valuation::SalePrice => "sale_price",
        }
    }

    pub fn parse(s: &str) -> Option<Valuation> {
        Valuation::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

sql_text_enum!(Valuation);

/// What the owner adds to the books' figures (`Serialize`, and the statement `Deserialize`: a
/// till's statement is saved by the main PC's host service).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ZakatInput {
    /// `YYYY-MM-DD`; today's business day when absent.
    pub day: Option<String>,
    pub valuation: Valuation,
    /// Bank and CCP balances.
    pub bank: Money,
    pub other_assets: Money,
    pub other_debts: Money,
    pub note: Option<String>,
}

/// The figures of a statement, computed by the backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ZakatPreview {
    pub day: String,
    pub hijri_date: HijriDate,
    /// In the language of the user who computed it.
    pub hijri_label: String,
    pub valuation: Valuation,
    pub stock_value: Money,
    /// The expected cash of the open drawers.
    pub cash: Money,
    pub bank: Money,
    pub other_assets: Money,
    pub receivables: Money,
    pub payables: Money,
    pub other_debts: Money,
    /// Assets minus debts.
    pub base: Money,
    pub nisab: Money,
    pub rate_bp: i32,
    pub zakat_due: Money,
    pub below_nisab: bool,
}

/// A saved statement (`ZK2026-000001`), never edited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ZakatStatement {
    pub id: String,
    pub number: String,
    pub preview: ZakatPreview,
    pub note: Option<String>,
    pub created_at: i64,
    pub user_name: String,
}

/// The stock on hand in the storage locations, at cost or at sale price. Quantities are
/// milli-units and prices per unit; the fraction of a centime per line is dropped (the
/// statement is indicative).
fn stock_value(conn: &Connection, valuation: Valuation) -> Result<Money, AppError> {
    let sql = match valuation {
        Valuation::Cost => {
            "SELECT COALESCE(SUM(l.qty * p.cost_price / 1000), 0)
             FROM stock_levels l JOIN products p ON p.id = l.product_id
             JOIN stock_locations s ON s.id = l.location_id
             WHERE s.kind = 'storage' AND l.qty > 0 AND p.track_stock = 1"
        }
        Valuation::SalePrice => {
            "SELECT COALESCE(SUM(l.qty * p.sell_price / 1000), 0)
             FROM stock_levels l JOIN products p ON p.id = l.product_id
             JOIN stock_locations s ON s.id = l.location_id
             WHERE s.kind = 'storage' AND l.qty > 0 AND p.track_stock = 1"
        }
    };
    Ok(conn.query_row(sql, [], |r| r.get(0))?)
}

/// The cash the open drawers should hold (a drawer is never counted below zero).
fn drawers(conn: &Connection) -> Result<Money, AppError> {
    let ids: Vec<String> = conn
        .prepare("SELECT id FROM cash_sessions WHERE status = 'open'")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut total = Money::ZERO;
    for id in ids {
        let expected = crate::cash::expected_cash(conn, &id)?.max(Money::ZERO);
        total = total
            .checked_add(expected)
            .ok_or_else(|| AppError::validation("cash", "amount_invalid"))?;
    }
    Ok(total)
}

/// The statement as it would be saved now, without saving it.
pub fn preview(
    conn: &Connection,
    ctx: &TillContext,
    input: &ZakatInput,
) -> Result<ZakatPreview, AppError> {
    packs::require(conn, Module::Zakat)?;
    for (field, m) in [
        ("bank", input.bank),
        ("otherAssets", input.other_assets),
        ("otherDebts", input.other_debts),
    ] {
        if m.is_negative() {
            return Err(AppError::validation(field, "amount_invalid"));
        }
    }
    let day = match crate::party::clean_day(input.day.as_deref(), "day")? {
        Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d")
            .map_err(|_| AppError::validation("day", "date_invalid"))?,
        None => ctx.business_date(),
    };
    let h = hijri(day)?;
    let stock = stock_value(conn, input.valuation)?;
    let cash = drawers(conn)?;
    let receivables = crate::customers::credit::receivables(conn)?;
    let payables = crate::suppliers::payables(conn)?;
    let sum = |xs: &[Money], field: &str| {
        Money::checked_sum(xs.iter().copied())
            .ok_or_else(|| AppError::validation(field, "amount_invalid"))
    };
    let assets = sum(
        &[stock, cash, input.bank, input.other_assets, receivables],
        "bank",
    )?;
    let debts = sum(&[payables, input.other_debts], "otherDebts")?;
    let base = assets
        .checked_sub(debts)
        .ok_or_else(|| AppError::validation("otherDebts", "amount_invalid"))?;
    let nisab = ctx.pack.zakat.nisab;
    let rate_bp = ctx.pack.zakat.rate_bp;
    let below = base < nisab;
    let due = if below {
        Money::ZERO
    } else {
        base.apply_rate(Rate(rate_bp))
            .ok_or_else(|| AppError::validation("bank", "amount_invalid"))?
    };
    Ok(ZakatPreview {
        day: day.format("%Y-%m-%d").to_string(),
        hijri_date: h,
        hijri_label: hijri_label(h, ctx.session.lang),
        valuation: input.valuation,
        stock_value: stock,
        cash,
        bank: input.bank,
        other_assets: input.other_assets,
        receivables,
        payables,
        other_debts: input.other_debts,
        base,
        nisab,
        rate_bp,
        zakat_due: due,
        below_nisab: below,
    })
}

/// Computes and saves the statement under the next `ZK` number, append-only.
pub fn save(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: ZakatInput,
) -> Result<ZakatStatement, AppError> {
    let p = preview(tx, ctx, &input)?;
    let note = crate::catalog::clean_opt(input.note.as_deref(), "note", NOTE_MAX)?;
    let number =
        crate::numbering::assign_custom(tx, "zakat_statement", "ZK", ctx.business_date())?.number;
    let id = new_id();
    tx.execute(
        "INSERT INTO zakat_statements (id, number, day, hijri_date, valuation, stock_value, cash, bank, other_assets,
                                       receivables, payables, other_debts, base, nisab, rate_bp, zakat_due, details,
                                       note, created_at, user_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        params![
            id,
            number,
            p.day,
            format!(
                "{:04}-{:02}-{:02}",
                p.hijri_date.year, p.hijri_date.month, p.hijri_date.day
            ),
            p.valuation,
            p.stock_value,
            p.cash,
            p.bank,
            p.other_assets,
            p.receivables,
            p.payables,
            p.other_debts,
            p.base,
            p.nisab,
            p.rate_bp,
            p.zakat_due,
            serde_json::to_string(&p)?,
            note,
            ctx.now,
            ctx.session.user_id
        ],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "zakat.saved",
            "zakat_statement",
            Some(&id),
            json!({ "number": number, "base": p.base, "due": p.zakat_due }),
        ),
    )?;
    get(tx, &id)
}

const SELECT: &str =
    "SELECT z.id, z.number, z.details, z.note, z.created_at, COALESCE(u.display_name, z.user_id)
     FROM zakat_statements z LEFT JOIN users u ON u.id = z.user_id";

/// A row of `zakat_statements` before its `details` JSON is read back into the preview.
struct Stored {
    id: String,
    number: String,
    details: String,
    note: Option<String>,
    created_at: i64,
    user_name: String,
}

fn map(r: &Row<'_>) -> rusqlite::Result<Stored> {
    Ok(Stored {
        id: r.get(0)?,
        number: r.get(1)?,
        details: r.get(2)?,
        note: r.get(3)?,
        created_at: r.get(4)?,
        user_name: r.get(5)?,
    })
}

fn decode(row: Stored) -> Result<ZakatStatement, AppError> {
    Ok(ZakatStatement {
        preview: serde_json::from_str(&row.details)?,
        id: row.id,
        number: row.number,
        note: row.note,
        created_at: row.created_at,
        user_name: row.user_name,
    })
}

/// One saved statement (`zakat_statement_not_found`).
pub fn get(conn: &Connection, id: &str) -> Result<ZakatStatement, AppError> {
    let row = conn
        .query_row(&format!("{SELECT} WHERE z.id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("zakat_statement"))?;
    decode(row)
}

/// The saved statements, newest first.
pub fn list(conn: &Connection) -> Result<Vec<ZakatStatement>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} ORDER BY z.created_at DESC, z.number DESC LIMIT ?1"
    ))?;
    let rows = stmt
        .query_map([PAGE_MAX], map)?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter().map(decode).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hijri_dates_follow_umm_al_qura() {
        let d = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        let h = |date| {
            let x = hijri(date).unwrap();
            (x.year, x.month, x.day)
        };
        assert_eq!(h(d(2026, 9, 28)), (1448, 4, 17));
        assert_eq!(h(d(2026, 3, 20)), (1447, 10, 1), "Aïd el-Fitr 1447");
        assert_eq!(h(d(2026, 6, 16)), (1448, 1, 1), "1 Muharram 1448");
        assert_eq!(h(d(2027, 2, 8)), (1448, 9, 1), "1 Ramadan 1448");
        assert_eq!(
            hijri_label(
                HijriDate {
                    year: 1448,
                    month: 4,
                    day: 17
                },
                Lang::Fr
            ),
            "17 Rabi' al-Thani 1448"
        );
        assert_eq!(
            hijri_label(
                HijriDate {
                    year: 1448,
                    month: 4,
                    day: 17
                },
                Lang::Ar
            ),
            "17 ربيع الآخر 1448"
        );
    }
}
