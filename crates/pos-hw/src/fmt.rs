//! Figures on paper: Western digits in every language (spec §10.2), plain spaces (thermal code
//! pages have no narrow no-break space), the currency only on the amount due.

use pos_core::lang::Lang;
use pos_core::money::{Money, Qty, Rate};

fn separators(lang: Lang) -> (char, char) {
    match lang {
        Lang::En => (',', '.'),
        Lang::Fr | Lang::Ar => (' ', ','),
    }
}

fn group(digits: &str, separator: char) -> String {
    let len = digits.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(c);
    }
    out
}

pub fn amount(value: Money, lang: Lang) -> String {
    let (group_sep, decimal) = separators(lang);
    let abs = value.0.unsigned_abs();
    let sign = if value.0 < 0 { "-" } else { "" };
    format!(
        "{sign}{}{decimal}{:02}",
        group(&(abs / 100).to_string(), group_sep),
        abs % 100
    )
}

pub fn qty(value: Qty, lang: Lang) -> String {
    let (_, decimal) = separators(lang);
    let abs = value.0.unsigned_abs();
    let sign = if value.0 < 0 { "-" } else { "" };
    let (whole, frac) = (abs / 1000, abs % 1000);
    if frac == 0 {
        format!("{sign}{whole}")
    } else {
        let digits = format!("{frac:03}");
        format!("{sign}{whole}{decimal}{}", digits.trim_end_matches('0'))
    }
}

pub fn rate(value: Rate, lang: Lang) -> String {
    let (_, decimal) = separators(lang);
    let bp = value.0.unsigned_abs();
    let (whole, frac) = (bp / 100, bp % 100);
    let number = if frac == 0 {
        whole.to_string()
    } else {
        let digits = format!("{frac:02}");
        format!("{whole}{decimal}{}", digits.trim_end_matches('0'))
    };
    match lang {
        Lang::Fr => format!("{number} %"),
        Lang::Ar | Lang::En => format!("{number}%"),
    }
}

pub fn currency(lang: Lang) -> &'static str {
    match lang {
        Lang::Ar => "دج",
        Lang::Fr | Lang::En => "DA",
    }
}

/// `dd/mm/yyyy HH:MM` in Africa/Algiers.
pub fn datetime(ms: i64) -> String {
    pos_core::time::algiers_datetime(ms)
        .format("%d/%m/%Y %H:%M")
        .to_string()
}

/// A stored `YYYY-MM-DD` date (a cheque's) as `dd/mm/yyyy`; anything else as it is.
pub fn date(iso: &str) -> String {
    let parts: Vec<&str> = iso.split('-').collect();
    match parts.as_slice() {
        [y, m, d]
            if y.len() == 4
                && m.len() == 2
                && d.len() == 2
                && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit())) =>
        {
            format!("{d}/{m}/{y}")
        }
        _ => iso.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_per_language_with_western_digits() {
        assert_eq!(amount(Money(450_000), Lang::Fr), "4 500,00");
        assert_eq!(amount(Money(450_000), Lang::Ar), "4 500,00");
        assert_eq!(amount(Money(450_000), Lang::En), "4,500.00");
        assert_eq!(amount(Money(-3_250), Lang::Fr), "-32,50");
        assert_eq!(amount(Money(5), Lang::En), "0.05");
        assert_eq!(amount(Money(12_345_678_900), Lang::Fr), "123 456 789,00");
        assert_eq!(
            amount(Money(i64::MIN), Lang::En).chars().next(),
            Some('-'),
            "never panics"
        );
    }

    #[test]
    fn quantities_trim_their_decimals() {
        assert_eq!(qty(Qty(2_000), Lang::Fr), "2");
        assert_eq!(qty(Qty(1_250), Lang::Fr), "1,25");
        assert_eq!(qty(Qty(125), Lang::En), "0.125");
        assert_eq!(qty(Qty(-1_000), Lang::Ar), "-1");
    }

    #[test]
    fn rates_follow_each_language_typography() {
        assert_eq!(rate(Rate(1_900), Lang::Fr), "19 %");
        assert_eq!(rate(Rate(1_900), Lang::En), "19%");
        assert_eq!(rate(Rate(950), Lang::Ar), "9,5%");
        assert_eq!(
            (currency(Lang::Fr), currency(Lang::Ar), currency(Lang::En)),
            ("DA", "دج", "DA")
        );
    }

    #[test]
    fn datetimes_are_algiers_local() {
        // 2026-09-28 12:00 Algiers (UTC+1)
        assert_eq!(datetime(1_790_593_200_000), "28/09/2026 12:00");
    }

    #[test]
    fn iso_dates_print_day_first() {
        assert_eq!(date("2026-09-28"), "28/09/2026");
        assert_eq!(date("28/09/2026"), "28/09/2026", "anything else as it is");
        assert_eq!(date("2026-9-28"), "2026-9-28");
    }
}
