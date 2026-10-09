//! English: hyphenated tens, no "and" inside a number (the "and" between dinars and centimes
//! is the `letters.dinarsAndCentimes` document string).

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

fn one(n: u64) -> &'static str {
    ONES[usize::try_from(n).unwrap_or(0) % ONES.len()]
}

fn ten(n: u64) -> &'static str {
    TENS[usize::try_from(n).unwrap_or(0) % TENS.len()]
}

fn below_1000(n: u64) -> String {
    let (h, rest) = (n / 100, n % 100);
    let tail = match rest {
        0 => String::new(),
        1..=19 => one(rest).to_owned(),
        _ if rest.is_multiple_of(10) => ten(rest / 10).to_owned(),
        _ => format!("{}-{}", ten(rest / 10), one(rest % 10)),
    };
    match (h, rest) {
        (0, _) => tail,
        (_, 0) => format!("{} hundred", one(h)),
        _ => format!("{} hundred {tail}", one(h)),
    }
}

fn number(n: u64) -> String {
    if n == 0 {
        return ONES[0].to_owned();
    }
    let billions = n / 1_000_000_000;
    let groups = [
        ((n / 1_000_000) % 1_000, "million"),
        ((n / 1_000) % 1_000, "thousand"),
        (n % 1_000, ""),
    ];
    let mut parts = Vec::new();
    if billions > 0 {
        parts.push(format!("{} billion", number(billions)));
    }
    for (count, scale) in groups {
        if count > 0 {
            let words = below_1000(count);
            parts.push(if scale.is_empty() {
                words
            } else {
                format!("{words} {scale}")
            });
        }
    }
    parts.join(" ")
}

pub(super) fn dinars(n: u64) -> String {
    format!("{} {}", number(n), if n == 1 { "dinar" } else { "dinars" })
}

pub(super) fn centimes(n: u64) -> String {
    format!(
        "{} {}",
        number(n),
        if n == 1 { "centime" } else { "centimes" }
    )
}
