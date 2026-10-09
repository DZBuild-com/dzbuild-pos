//! Values people type in spreadsheets (spec §12 import rules, UX-014): Arabic-Indic digits,
//! spaces as thousands separators, a decimal comma or point — and never a guess between a
//! decimal separator and a thousands separator. `Ok(None)` is an empty cell; `Err` is a problem
//! code (`imports::spec::PROBLEM_CODES`) the dry run shows in the user's language.

use crate::catalog::normalize::normalize;
use crate::money::{Money, Qty};
use crate::party::western_digits;
use crate::table::TagDomain;
use chrono::NaiveDate;
use std::collections::BTreeMap;

pub type Parsed<T> = Result<Option<T>, &'static str>;

const CURRENCY_SUFFIXES: [&str; 3] = ["dzd", "da", "دج"];

/// Western digits, no spaces or group marks, the Arabic decimal mark as a comma, the currency
/// suffix removed.
fn cleaned(text: &str) -> String {
    let western = western_digits(text.trim()).to_lowercase();
    let mut s: String = western
        .chars()
        .filter(|c| !matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '\u{66c}'))
        .map(|c| if c == '\u{66b}' { ',' } else { c })
        .collect();
    for suffix in CURRENCY_SUFFIXES {
        if let Some(rest) = s.strip_suffix(suffix) {
            s = rest.to_owned();
            break;
        }
    }
    s
}

/// `(negative, whole digits, decimal digits)` of a cleaned number with at most one separator.
fn parts(s: &str) -> Result<(bool, &str, &str), &'static str> {
    let (negative, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    if body.is_empty()
        || !body
            .chars()
            .all(|c| c.is_ascii_digit() || c == ',' || c == '.')
    {
        return Err("number_invalid");
    }
    let separators: Vec<usize> = body
        .char_indices()
        .filter(|(_, c)| *c == ',' || *c == '.')
        .map(|(i, _)| i)
        .collect();
    match separators.as_slice() {
        [] => Ok((negative, body, "")),
        [at] => {
            let (whole, frac) = (&body[..*at], &body[at + 1..]);
            if frac.is_empty() {
                return Err("number_invalid");
            }
            Ok((negative, if whole.is_empty() { "0" } else { whole }, frac))
        }
        _ => Err("ambiguous_separator"),
    }
}

/// `whole.frac` as an integer of `scale` decimals (the fraction already fits `scale`).
fn scaled(negative: bool, whole: &str, frac: &str, scale: usize) -> Result<i64, &'static str> {
    let mut digits = String::with_capacity(whole.len() + scale);
    digits.push_str(whole);
    digits.push_str(frac);
    for _ in frac.len()..scale {
        digits.push('0');
    }
    let value: i64 = digits.parse().map_err(|_| "number_invalid")?;
    Ok(if negative { -value } else { value })
}

/// A signed amount in centimes: at most 2 decimals; exactly 3 digits after the only separator
/// is refused (a thousands separator or a decimal? the file must say).
pub fn money(text: &str) -> Parsed<Money> {
    let s = cleaned(text);
    if s.is_empty() {
        return Ok(None);
    }
    let (negative, whole, frac) = parts(&s)?;
    match frac.len() {
        0..=2 => Ok(Some(Money(scaled(negative, whole, frac, 2)?))),
        3 => Err("ambiguous_separator"),
        _ => Err("too_many_decimals"),
    }
}

/// A quantity in milli-units for a unit with `unit_decimals` decimals.
pub fn qty(text: &str, unit_decimals: u8) -> Parsed<Qty> {
    let s = cleaned(text);
    if s.is_empty() {
        return Ok(None);
    }
    let (negative, whole, frac) = parts(&s)?;
    let allowed = usize::from(unit_decimals.min(3));
    match frac.len() {
        n if n <= allowed => Ok(Some(Qty(scaled(negative, whole, frac, 3)?))),
        3 => Err("ambiguous_separator"),
        _ => Err("too_many_decimals"),
    }
}

/// A percentage in basis points (0–100 %), at most 2 decimals. A bare `0,x` (no `%`) is a ratio
/// read from a percent-formatted cell (`0.19` → 19 %), since the reader never sees the cell's
/// format; a written `%` means the number is already a percentage (`0,5 %` → 0.5 %). The reader
/// cannot tell a percent-formatted cell of 100 % from a typed 1 either: both arrive as `1` and
/// read as 1 %.
pub fn percent_bp(text: &str) -> Parsed<i64> {
    let s = cleaned(text);
    let explicit = s.ends_with('%');
    let s = s.trim_end_matches('%');
    if s.is_empty() {
        return Ok(None);
    }
    let (negative, whole, frac) = parts(s)?;
    if negative {
        return Err("negative");
    }
    let bp = if !explicit && whole.trim_start_matches('0').is_empty() && !frac.is_empty() {
        if frac.len() > 4 {
            return Err("too_many_decimals");
        }
        scaled(false, "", frac, 4)?
    } else {
        if frac.len() > 2 {
            return Err("too_many_decimals");
        }
        scaled(false, whole, frac, 2)?
    };
    if bp > 10_000 {
        Err("number_invalid")
    } else {
        Ok(Some(bp))
    }
}

pub fn int(text: &str) -> Parsed<i64> {
    let s = cleaned(text);
    if s.is_empty() {
        return Ok(None);
    }
    s.parse::<i64>().map(Some).map_err(|_| "number_invalid")
}

pub fn boolean(text: &str) -> Parsed<bool> {
    let s = normalize(text);
    match s.as_str() {
        "" => Ok(None),
        "oui" | "o" | "yes" | "y" | "true" | "vrai" | "1" | "x" | "نعم" => Ok(Some(true)),
        "non" | "n" | "no" | "false" | "faux" | "0" | "لا" => Ok(Some(false)),
        _ => Err("bool_invalid"),
    }
}

/// A day as `YYYY-MM-DD` from `YYYY-MM-DD`, `DD/MM/YYYY`, `DD-MM-YYYY` or `DD.MM.YYYY`.
pub fn date(text: &str) -> Parsed<String> {
    let s = western_digits(text.trim());
    if s.is_empty() {
        return Ok(None);
    }
    for format in ["%Y-%m-%d", "%d/%m/%Y", "%d-%m-%Y", "%d.%m.%Y"] {
        if let Ok(day) = NaiveDate::parse_from_str(&s, format) {
            return Ok(Some(day.format("%Y-%m-%d").to_string()));
        }
    }
    Err("date_invalid")
}

/// The error values a spreadsheet formula leaves in a cell (`#N/A` of a failed VLOOKUP, …), as
/// `pos_docs::import` reads them. The number, boolean and date parsers already refuse them; a
/// text, code or list field must refuse them with `value_invalid` rather than store them.
pub const EXCEL_ERRORS: [&str; 10] = [
    "#N/A",
    "#REF!",
    "#VALUE!",
    "#DIV/0!",
    "#NAME?",
    "#NULL!",
    "#NUM!",
    "#DATA!",
    "#GETTING_DATA",
    "#SPILL!",
];

/// `text` is exactly a spreadsheet error value (`EXCEL_ERRORS`, any case, surrounding spaces
/// ignored).
pub fn excel_error(text: &str) -> bool {
    let text = text.trim();
    EXCEL_ERRORS.iter().any(|e| e.eq_ignore_ascii_case(text))
}

/// Several codes in one cell (barcodes): separated by `|`, `;`, `,` or spaces.
pub fn list(text: &str) -> Vec<String> {
    western_digits(text)
        .split(|c: char| matches!(c, '|' | ';' | ',') || c.is_whitespace())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A value of a closed list, typed as its code or as its label in any language. `known` maps
/// `"<domain>:<normalised code or label>"` to the code (built by `pos_docs::import::choice_labels`).
pub fn choice(text: &str, domain: TagDomain, known: &BTreeMap<String, String>) -> Parsed<String> {
    let key = normalize(text);
    if key.is_empty() {
        return Ok(None);
    }
    known
        .get(&format!("{}:{key}", domain.as_str()))
        .cloned()
        .map(Some)
        .ok_or("choice_invalid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn money_never_guesses_a_thousands_separator() {
        assert_eq!(money("1190"), Ok(Some(Money(119_000))));
        assert_eq!(money("1190,5"), Ok(Some(Money(119_050))));
        assert_eq!(money("1190.50"), Ok(Some(Money(119_050))));
        assert_eq!(money("1 190,50 DA"), Ok(Some(Money(119_050))));
        assert_eq!(money("1\u{a0}190,50"), Ok(Some(Money(119_050))));
        assert_eq!(money("١٬١٩٠٫٥٠ دج"), Ok(Some(Money(119_050))));
        assert_eq!(money("-250"), Ok(Some(Money(-25_000))));
        assert_eq!(money("  "), Ok(None));
        // The ambiguous forms are refused, never read as thousands.
        assert_eq!(money("1,234"), Err("ambiguous_separator"));
        assert_eq!(money("1.234"), Err("ambiguous_separator"));
        assert_eq!(money("1.234,56"), Err("ambiguous_separator"));
        assert_eq!(money("1,234.56"), Err("ambiguous_separator"));
        assert_eq!(money("1.234.567"), Err("ambiguous_separator"));
        assert_eq!(money("12,3456"), Err("too_many_decimals"));
        assert_eq!(money("douze"), Err("number_invalid"));
        assert_eq!(money("12-5"), Err("number_invalid"));
    }

    #[test]
    fn quantities_take_three_decimals_only_for_weighed_units() {
        assert_eq!(qty("1,250", 3), Ok(Some(Qty(1_250))));
        assert_eq!(qty("1,250", 0), Err("ambiguous_separator"));
        assert_eq!(qty("12", 0), Ok(Some(Qty(12_000))));
        assert_eq!(qty("1,5", 0), Err("too_many_decimals"));
        assert_eq!(qty("1,5", 1), Ok(Some(Qty(1_500))));
        assert_eq!(qty("0,125", 3), Ok(Some(Qty(125))));
    }

    #[test]
    fn percents_booleans_dates_and_lists() {
        assert_eq!(percent_bp("19"), Ok(Some(1_900)));
        assert_eq!(percent_bp("19 %"), Ok(Some(1_900)));
        assert_eq!(
            percent_bp("0.19"),
            Ok(Some(1_900)),
            "a percent-formatted Excel cell"
        );
        assert_eq!(percent_bp("12,5"), Ok(Some(1_250)));
        assert_eq!(percent_bp("120"), Err("number_invalid"));
        // A written '%' says the number is already a percentage: never a ratio.
        assert_eq!(percent_bp("0,5 %"), Ok(Some(50)));
        assert_eq!(percent_bp("0,19 %"), Ok(Some(19)));
        assert_eq!(percent_bp("0.5%"), Ok(Some(50)));
        assert_eq!(percent_bp("0,125 %"), Err("too_many_decimals"));
        // A percent-formatted cell of 100 % arrives as "1" and reads as 1 % (see `percent_bp`).
        assert_eq!(percent_bp("1"), Ok(Some(100)));
        for yes in ["oui", "Yes", "TRUE", "1", "x", "نعم"] {
            assert_eq!(boolean(yes), Ok(Some(true)), "{yes}");
        }
        for no in ["non", "no", "false", "0", "لا"] {
            assert_eq!(boolean(no), Ok(Some(false)), "{no}");
        }
        assert_eq!(boolean("peut-être"), Err("bool_invalid"));
        assert_eq!(date("28/09/2026"), Ok(Some("2026-09-28".into())));
        assert_eq!(date("2026-09-28"), Ok(Some("2026-09-28".into())));
        assert_eq!(date("28.09.2026"), Ok(Some("2026-09-28".into())));
        assert_eq!(date("31/02/2026"), Err("date_invalid"));
        assert_eq!(
            list("6130000000015 | 6130000000022;6130000000039"),
            vec!["6130000000015", "6130000000022", "6130000000039"]
        );
        assert_eq!(int("٣٠"), Ok(Some(30)));
    }

    #[test]
    fn excel_error_cells_are_recognised_and_never_read_as_values() {
        for error in [
            "#N/A", "#REF!", "#VALUE!", "#DIV/0!", "#NAME?", "#NULL!", "#NUM!", " #n/a ",
        ] {
            assert!(excel_error(error), "{error}");
            assert_eq!(money(error), Err("number_invalid"), "{error}");
            assert_eq!(qty(error, 3), Err("number_invalid"), "{error}");
            assert_eq!(percent_bp(error), Err("number_invalid"), "{error}");
            assert_eq!(int(error), Err("number_invalid"), "{error}");
            assert_eq!(boolean(error), Err("bool_invalid"), "{error}");
            assert_eq!(date(error), Err("date_invalid"), "{error}");
        }
        for value in ["", "N/A", "#12", "Réf #REF", "6130000000015"] {
            assert!(!excel_error(value), "{value}");
        }
    }

    proptest! {
        #[test]
        fn a_written_amount_reads_back(centimes in -10_000_000_000_i64..10_000_000_000) {
            let comma = format!(
                "{}{},{:02}",
                if centimes < 0 { "-" } else { "" },
                centimes.unsigned_abs() / 100,
                centimes.unsigned_abs() % 100
            );
            prop_assert_eq!(money(&comma), Ok(Some(Money(centimes))));
            prop_assert_eq!(money(&comma.replace(',', ".")), Ok(Some(Money(centimes))));
        }
    }
}
