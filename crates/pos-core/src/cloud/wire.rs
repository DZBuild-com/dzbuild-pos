//! Wire formats of the DZBuild API (Backend Brief): amounts, quantities, rates and times as
//! exact decimal text, never `f64`. Every amount crosses the wire through [`ApiMoney`], whose unit
//! is the single constant [`API_MONEY_UNIT`]. An amount is written by the single encoder
//! [`money_wire`] as a decimal string (`"4500.50"`), which any backend reads exactly, and read back
//! from the raw text of the response, a number or a string, so `4500.50` is 450 050 centimes.

use crate::money::{Money, Qty, Rate};
use crate::time::ALGIERS_OFFSET_MS;
use chrono::{DateTime, FixedOffset, NaiveDateTime};
use serde::de::Error as _;
use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoneyUnit {
    /// Dinars with up to two decimals: 450 050 centimes are `4500.5`.
    Dinars,
    /// Whole centimes: 450 050 centimes are `450050`.
    Centimes,
}

/// The unit of every amount the DZBuild API sends or receives. The Backend Brief's example
/// (`"unit_price": 4500` for a 4 500 DA bag) says dinars. If DZBuild settles on centimes, flip
/// this constant and the two examples of `the_api_money_unit_is_pinned`; nothing else changes.
pub const API_MONEY_UNIT: MoneyUnit = MoneyUnit::Dinars;

/// `value / 10^decimals` as the shortest exact decimal: (450050, 2) → `4500.5`, (-100000, 2) →
/// `-1000`.
fn decimal_text(value: i64, decimals: u32) -> String {
    let scale = 10_i128.pow(decimals);
    let wide = i128::from(value);
    let sign = if wide < 0 { "-" } else { "" };
    let abs = wide.abs();
    let (whole, frac) = (abs / scale, abs % scale);
    if frac == 0 {
        return format!("{sign}{whole}");
    }
    let digits = format!("{frac:0width$}", width = decimals as usize);
    format!("{sign}{whole}.{}", digits.trim_end_matches('0'))
}

/// Parses `-?digits(.digits)?` with at most `max_decimals` decimals into `value × 10^max`.
/// No exponent, no `+`, no leading or trailing dot, no spaces; `None` on overflow.
fn parse_decimal(text: &str, max_decimals: u32) -> Option<i64> {
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (whole, frac) = match body.split_once('.') {
        Some((w, f)) => (w, Some(f)),
        None => (body, None),
    };
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let frac = frac.unwrap_or("");
    if body.contains('.') && frac.is_empty() {
        return None;
    }
    if frac.len() > max_decimals as usize || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut value: i64 = whole.parse().ok()?;
    for _ in 0..max_decimals {
        value = value.checked_mul(10)?;
    }
    let mut frac_value: i64 = if frac.is_empty() {
        0
    } else {
        frac.parse().ok()?
    };
    for _ in frac.len()..max_decimals as usize {
        frac_value = frac_value.checked_mul(10)?;
    }
    let total = value.checked_add(frac_value)?;
    Some(if negative { -total } else { total })
}

/// The single encoder of the amounts sent to DZBuild: dinars as a JSON string with exactly two
/// decimals (`"4500.50"`, `"0.07"`, `"-1000.00"`). A JSON number with decimals is a float to most
/// parsers (JavaScript's among them); a string is read exactly, whatever the amount.
pub fn money_wire(amount: Money) -> String {
    let wide = i128::from(amount.0);
    let sign = if wide < 0 { "-" } else { "" };
    let abs = wide.abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

pub fn money_text(amount: Money, unit: MoneyUnit) -> String {
    match unit {
        MoneyUnit::Dinars => decimal_text(amount.0, 2),
        MoneyUnit::Centimes => amount.0.to_string(),
    }
}

pub fn parse_money(text: &str, unit: MoneyUnit) -> Option<Money> {
    match unit {
        MoneyUnit::Dinars => parse_decimal(text, 2).map(Money),
        MoneyUnit::Centimes => parse_decimal(text, 0).map(Money),
    }
}

/// Units with up to three decimals (1 piece = `1`, 1.25 kg = `1.25`).
pub fn qty_text(qty: Qty) -> String {
    decimal_text(qty.0, 3)
}

pub fn parse_qty(text: &str) -> Option<Qty> {
    parse_decimal(text, 3).map(Qty)
}

/// A rate in percent: 1 900 bp → `19`, 950 bp → `9.5`.
pub fn rate_text(rate: Rate) -> String {
    decimal_text(i64::from(rate.0), 2)
}

fn parse_rate(text: &str) -> Option<Rate> {
    parse_decimal(text, 2)
        .and_then(|bp| i32::try_from(bp).ok())
        .map(Rate)
}

/// Whole pieces when `qty` holds no fraction of a unit (DZBuild counts pieces).
pub fn whole_pieces(qty: Qty) -> Option<i64> {
    (qty.0 % crate::money::QTY_SCALE == 0).then_some(qty.0 / crate::money::QTY_SCALE)
}

/// Africa/Algiers, UTC+1 all year. `east_opt` only refuses offsets beyond ±24 h, so the UTC
/// fallback is never taken; it keeps the function total without `unwrap`.
fn algiers() -> FixedOffset {
    use chrono::{Offset, Utc};
    FixedOffset::east_opt((ALGIERS_OFFSET_MS / 1_000) as i32).unwrap_or_else(|| Utc.fix())
}

/// RFC 3339 in Algiers time: `2026-09-28T11:42:07+01:00` (seconds; milliseconds dropped).
pub fn iso_time(ms: i64) -> String {
    DateTime::from_timestamp_millis(ms)
        .map(|d| {
            d.with_timezone(&algiers())
                .format("%Y-%m-%dT%H:%M:%S%:z")
                .to_string()
        })
        .unwrap_or_default()
}

/// RFC 3339 with any offset, or DZBuild's store format `YYYY-MM-DD HH:MM:SS` in Algiers time.
pub fn parse_time(text: &str) -> Option<i64> {
    if let Ok(d) = DateTime::parse_from_rfc3339(text) {
        return Some(d.timestamp_millis());
    }
    let local = NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(local.and_utc().timestamp_millis() - ALGIERS_OFFSET_MS)
}

/// The raw text of a JSON number (or of a decimal sent as a string).
fn raw_number<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let raw: Box<RawValue> = Box::deserialize(d)?;
    let text = raw.get().trim();
    let text = text
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text);
    Ok(text.to_owned())
}

fn write_raw<S: Serializer>(text: String, s: S) -> Result<S::Ok, S::Error> {
    RawValue::from_string(text)
        .map_err(S::Error::custom)?
        .serialize(s)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiMoney(pub Money);

impl Serialize for ApiMoney {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&money_wire(self.0))
    }
}

impl<'de> Deserialize<'de> for ApiMoney {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = raw_number(d)?;
        parse_money(&text, API_MONEY_UNIT)
            .map(ApiMoney)
            .ok_or_else(|| D::Error::custom("amount is not an exact decimal in the API unit"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiQty(pub Qty);

impl Serialize for ApiQty {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        write_raw(qty_text(self.0), s)
    }
}

impl<'de> Deserialize<'de> for ApiQty {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = raw_number(d)?;
        parse_qty(&text)
            .map(ApiQty)
            .ok_or_else(|| D::Error::custom("quantity is not an exact decimal"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiRate(pub Rate);

impl Serialize for ApiRate {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        write_raw(rate_text(self.0), s)
    }
}

impl<'de> Deserialize<'de> for ApiRate {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = raw_number(d)?;
        parse_rate(&text)
            .map(ApiRate)
            .ok_or_else(|| D::Error::custom("rate is not a percentage"))
    }
}

/// Unix milliseconds UTC, written as RFC 3339 in Algiers time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiTime(pub i64);

impl Serialize for ApiTime {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&iso_time(self.0))
    }
}

impl<'de> Deserialize<'de> for ApiTime {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        parse_time(&text)
            .map(ApiTime)
            .ok_or_else(|| D::Error::custom("time is neither RFC 3339 nor YYYY-MM-DD HH:MM:SS"))
    }
}

/// A remote id, whether DZBuild sends `141` or `"dev_7f3a"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct IdString(pub String);

impl<'de> Deserialize<'de> for IdString {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Either {
            Int(i64),
            Text(String),
        }
        match Either::deserialize(d)? {
            Either::Int(n) => Ok(IdString(n.to_string())),
            Either::Text(s) if !s.is_empty() && s.len() <= 128 => Ok(IdString(s)),
            Either::Text(_) => Err(D::Error::custom("empty or oversized id")),
        }
    }
}

/// `#[serde(deserialize_with = "de_id")]` for a `String` field DZBuild may send as a number.
pub fn de_id<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    IdString::deserialize(d).map(|id| id.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Line {
        unit_price: ApiMoney,
        discount: ApiMoney,
        quantity: ApiQty,
        tax_rate: ApiRate,
    }

    #[test]
    fn the_api_money_unit_is_pinned() {
        // Dinars, as DZBuild's own amounts are: "4500" or "4500.00" read as a 4 500 DA bag. The
        // POS writes every amount as a decimal string with two decimals ([`money_wire`]).
        assert_eq!(API_MONEY_UNIT, MoneyUnit::Dinars);
        assert_eq!(money_wire(Money(7)), "0.07");
        assert_eq!(money_wire(Money(1_234_567_891)), "12345678.91");
        assert_eq!(money_wire(Money(-100_000)), "-1000.00");
        assert_eq!(money_wire(Money(-5)), "-0.05");
        let line = Line {
            unit_price: ApiMoney(Money(450_000)),
            discount: ApiMoney(Money(50_000)),
            quantity: ApiQty(Qty(1_000)),
            tax_rate: ApiRate(Rate(1_900)),
        };
        assert_eq!(
            serde_json::to_string(&line).unwrap(),
            r#"{"unit_price":"4500.00","discount":"500.00","quantity":1,"tax_rate":19}"#,
            "amounts leave as decimal strings: exact for any backend"
        );
        let back: Line = serde_json::from_str(
            r#"{"unit_price":4500,"discount":500,"quantity":1,"tax_rate":19}"#,
        )
        .unwrap();
        assert_eq!(back, line);
    }

    #[test]
    fn amounts_are_exact_decimal_text() {
        assert_eq!(money_text(Money(450_050), MoneyUnit::Dinars), "4500.5");
        assert_eq!(money_text(Money(1), MoneyUnit::Dinars), "0.01");
        assert_eq!(money_text(Money(-100_000), MoneyUnit::Dinars), "-1000");
        assert_eq!(money_text(Money(450_050), MoneyUnit::Centimes), "450050");
        assert_eq!(
            parse_money("4500.50", MoneyUnit::Dinars),
            Some(Money(450_050))
        );
        assert_eq!(parse_money("-0.5", MoneyUnit::Dinars), Some(Money(-50)));
        assert_eq!(parse_money("4500", MoneyUnit::Centimes), Some(Money(4_500)));
        for refused in [
            "4500.555", "1e3", "+4", ".5", "5.", "", "-", "4 500", "0x10", "1.2.3", "NaN",
        ] {
            assert_eq!(parse_money(refused, MoneyUnit::Dinars), None, "{refused:?}");
        }
        assert_eq!(
            parse_money("4500.5", MoneyUnit::Centimes),
            None,
            "centimes are whole"
        );
        assert_eq!(
            parse_money("99999999999999999999", MoneyUnit::Dinars),
            None,
            "overflow"
        );
        let quoted: ApiMoney = serde_json::from_str("\"4500.50\"").unwrap();
        assert_eq!(
            quoted,
            ApiMoney(Money(450_050)),
            "a decimal sent as a string is read exactly"
        );
        assert!(serde_json::from_str::<ApiMoney>("4500.555").is_err());
        assert!(serde_json::from_str::<ApiMoney>("true").is_err());
    }

    #[test]
    fn quantities_rates_and_pieces() {
        assert_eq!(qty_text(Qty(1_250)), "1.25");
        assert_eq!(qty_text(Qty(-3_000)), "-3");
        assert_eq!(parse_qty("0.125"), Some(Qty(125)));
        assert_eq!(parse_qty("0.1255"), None);
        assert_eq!(rate_text(Rate(950)), "9.5");
        assert_eq!(rate_text(Rate(0)), "0");
        assert_eq!(whole_pieces(Qty(24_000)), Some(24));
        assert_eq!(
            whole_pieces(Qty(1_500)),
            None,
            "a weight is never a number of pieces"
        );
        let rate: ApiRate = serde_json::from_str("19").unwrap();
        assert_eq!(rate, ApiRate(Rate(1_900)));
    }

    #[test]
    fn times_are_algiers_rfc3339_and_the_store_format_is_read() {
        // 2026-09-28 11:42:07 in Algiers = 10:42:07 UTC.
        let ms = 1_790_592_127_000;
        assert_eq!(iso_time(ms), "2026-09-28T11:42:07+01:00");
        assert_eq!(parse_time("2026-09-28T11:42:07+01:00"), Some(ms));
        assert_eq!(parse_time("2026-09-28T10:42:07Z"), Some(ms));
        assert_eq!(
            parse_time("2026-09-28 11:42:07"),
            Some(ms),
            "store server local time"
        );
        assert_eq!(parse_time("28/09/2026"), None);
        let t: ApiTime = serde_json::from_str("\"2026-09-28 11:42:07\"").unwrap();
        assert_eq!(t, ApiTime(ms));
        assert_eq!(
            serde_json::to_string(&ApiTime(ms)).unwrap(),
            "\"2026-09-28T11:42:07+01:00\""
        );
    }

    #[test]
    fn remote_ids_are_strings_whatever_the_json() {
        #[derive(Deserialize)]
        struct Row {
            id: IdString,
        }
        let a: Row = serde_json::from_str(r#"{"id":141}"#).unwrap();
        let b: Row = serde_json::from_str(r#"{"id":"dev_7f3a"}"#).unwrap();
        assert_eq!((a.id.0.as_str(), b.id.0.as_str()), ("141", "dev_7f3a"));
        assert!(serde_json::from_str::<Row>(r#"{"id":1.5}"#).is_err());
    }

    #[test]
    fn de_id_reads_numbers_and_strings() {
        #[derive(Deserialize)]
        struct Store {
            #[serde(deserialize_with = "de_id")]
            store_id: String,
        }
        let s: Store = serde_json::from_str(r#"{"store_id":141}"#).unwrap();
        assert_eq!(s.store_id, "141");
    }

    proptest! {
        #[test]
        fn money_round_trips_in_both_units(centimes in -1_000_000_000_000_i64..1_000_000_000_000) {
            for unit in [MoneyUnit::Dinars, MoneyUnit::Centimes] {
                let text = money_text(Money(centimes), unit);
                prop_assert_eq!(parse_money(&text, unit), Some(Money(centimes)));
            }
            let json = serde_json::to_string(&ApiMoney(Money(centimes))).unwrap();
            prop_assert_eq!(serde_json::from_str::<ApiMoney>(&json).unwrap(), ApiMoney(Money(centimes)));
        }

        #[test]
        fn quantities_round_trip(milli in -1_000_000_000_i64..1_000_000_000) {
            prop_assert_eq!(parse_qty(&qty_text(Qty(milli))), Some(Qty(milli)));
        }
    }
}
