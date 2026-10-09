//! French: traditional spelling (hyphens below 100 only), "et" in 21…71, "quatre-vingts" and
//! "cents" plural only when they end the number before a noun (not before "mille"), "mille"
//! invariable, "million(s)" and "milliard(s)" nouns followed by "de" when nothing follows.

const UNITS: [&str; 20] = [
    "zéro", "un", "deux", "trois", "quatre", "cinq", "six", "sept", "huit", "neuf", "dix", "onze",
    "douze", "treize", "quatorze", "quinze", "seize", "dix-sept", "dix-huit", "dix-neuf",
];
const TENS: [&str; 10] = [
    "",
    "",
    "vingt",
    "trente",
    "quarante",
    "cinquante",
    "soixante",
    "soixante",
    "quatre-vingt",
    "quatre-vingt",
];

fn unit(n: u64) -> &'static str {
    UNITS[usize::try_from(n).unwrap_or(0) % UNITS.len()]
}

/// 0..=99; `last` when only a noun follows (plural "quatre-vingts").
fn below_100(n: u64, last: bool) -> String {
    let (t, u) = (n / 10, n % 10);
    let tens = TENS[usize::try_from(t).unwrap_or(0) % TENS.len()];
    match n {
        0..=19 => unit(n).to_owned(),
        71 => "soixante et onze".to_owned(),
        _ if t == 7 || t == 9 => format!("{tens}-{}", unit(10 + u)),
        80 if last => "quatre-vingts".to_owned(),
        _ if u == 0 => tens.to_owned(),
        _ if u == 1 && t != 8 => format!("{tens} et un"),
        _ => format!("{tens}-{}", unit(u)),
    }
}

/// 1..=999.
fn below_1000(n: u64, last: bool) -> String {
    let (h, rest) = (n / 100, n % 100);
    let hundreds = match h {
        0 => None,
        1 => Some("cent".to_owned()),
        _ if rest == 0 && last => Some(format!("{} cents", unit(h))),
        _ => Some(format!("{} cent", unit(h))),
    };
    match (hundreds, rest) {
        (None, _) => below_100(rest, last),
        (Some(h), 0) => h,
        (Some(h), _) => format!("{h} {}", below_100(rest, last)),
    }
}

fn number(n: u64) -> String {
    if n == 0 {
        return UNITS[0].to_owned();
    }
    let billions = n / 1_000_000_000;
    let millions = (n / 1_000_000) % 1_000;
    let thousands = (n / 1_000) % 1_000;
    let rest = n % 1_000;
    let mut parts = Vec::new();
    if billions > 0 {
        parts.push(match billions {
            1 => "un milliard".to_owned(),
            _ => format!("{} milliards", number(billions)),
        });
    }
    if millions > 0 {
        parts.push(match millions {
            1 => "un million".to_owned(),
            _ => format!("{} millions", below_1000(millions, true)),
        });
    }
    if thousands > 0 {
        parts.push(match thousands {
            1 => "mille".to_owned(),
            _ => format!("{} mille", below_1000(thousands, false)),
        });
    }
    if rest > 0 {
        parts.push(below_1000(rest, true));
    }
    parts.join(" ")
}

pub(super) fn dinars(n: u64) -> String {
    let noun = if n <= 1 { "dinar" } else { "dinars" };
    let de = n >= 1_000_000 && n.is_multiple_of(1_000_000);
    format!("{}{} {noun}", number(n), if de { " de" } else { "" })
}

pub(super) fn centimes(n: u64) -> String {
    format!(
        "{} {}",
        number(n),
        if n <= 1 { "centime" } else { "centimes" }
    )
}
