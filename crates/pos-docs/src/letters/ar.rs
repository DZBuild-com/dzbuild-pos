//! Arabic (تفقيط, research 04 §8.2): the counted noun agrees with the number: 1 → "دينار واحد",
//! 2 → dual "ديناران", 3-10 → plural "دنانير" (number in the feminine form), 11-99 → accusative
//! singular "ديناراً", hundreds and the rest → genitive singular "دينار". Thousands, millions
//! and billions are counted nouns themselves; a dual or accusative that directly precedes
//! another noun takes its construct form ("ألفا دينار", "أحد عشر ألف دينار", "مائتا ألف").
//! Units are joined with " و" (the conjunction written against the next word).

const ONES: [&str; 11] = [
    "",
    "واحد",
    "اثنان",
    "ثلاثة",
    "أربعة",
    "خمسة",
    "ستة",
    "سبعة",
    "ثمانية",
    "تسعة",
    "عشرة",
];
const TENS: [&str; 10] = [
    "",
    "عشرة",
    "عشرون",
    "ثلاثون",
    "أربعون",
    "خمسون",
    "ستون",
    "سبعون",
    "ثمانون",
    "تسعون",
];
const HUNDREDS: [&str; 10] = [
    "",
    "مائة",
    "مائتان",
    "ثلاثمائة",
    "أربعمائة",
    "خمسمائة",
    "ستمائة",
    "سبعمائة",
    "ثمانمائة",
    "تسعمائة",
];

/// Forms of a masculine counted noun.
struct Noun {
    singular: &'static str,
    dual: &'static str,
    dual_construct: &'static str,
    plural: &'static str,
    accusative: &'static str,
}

const THOUSAND: Noun = Noun {
    singular: "ألف",
    dual: "ألفان",
    dual_construct: "ألفا",
    plural: "آلاف",
    accusative: "ألفاً",
};
const MILLION: Noun = Noun {
    singular: "مليون",
    dual: "مليونان",
    dual_construct: "مليونا",
    plural: "ملايين",
    accusative: "مليوناً",
};
const BILLION: Noun = Noun {
    singular: "مليار",
    dual: "ملياران",
    dual_construct: "مليارا",
    plural: "مليارات",
    accusative: "ملياراً",
};
const DINAR: Noun = Noun {
    singular: "دينار",
    dual: "ديناران",
    dual_construct: "دينارا",
    plural: "دنانير",
    accusative: "ديناراً",
};
const CENTIME: Noun = Noun {
    singular: "سنتيم",
    dual: "سنتيمان",
    dual_construct: "سنتيما",
    plural: "سنتيمات",
    accusative: "سنتيماً",
};

fn pick(table: &[&'static str], n: u64) -> &'static str {
    table[usize::try_from(n).unwrap_or(0) % table.len()]
}

fn join(parts: Vec<String>) -> String {
    parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" و")
}

fn below_100(n: u64) -> String {
    match n {
        0 => String::new(),
        1..=10 => pick(&ONES, n).to_owned(),
        11 => "أحد عشر".to_owned(),
        12 => "اثنا عشر".to_owned(),
        13..=19 => format!("{} عشر", pick(&ONES, n - 10)),
        _ if n.is_multiple_of(10) => pick(&TENS, n / 10).to_owned(),
        _ => format!("{} و{}", pick(&ONES, n % 10), pick(&TENS, n / 10)),
    }
}

/// 1..=999. Round hundreds always precede a noun here, so 200 is the construct "مائتا".
fn below_1000(n: u64) -> String {
    let (h, rest) = (n / 100, n % 100);
    let hundred = if h == 2 && rest == 0 {
        "مائتا"
    } else {
        pick(&HUNDREDS, h)
    };
    join(vec![hundred.to_owned(), below_100(rest)])
}

/// `c` (≥ 1) units of the scale `noun`; `construct` when another noun follows directly.
fn counted(c: u64, noun: &Noun, construct: bool) -> String {
    match c {
        1 => noun.singular.to_owned(),
        2 if construct => noun.dual_construct.to_owned(),
        2 => noun.dual.to_owned(),
        _ => {
            let form = match c % 100 {
                3..=10 => noun.plural,
                11..=99 if !construct => noun.accusative,
                _ => noun.singular,
            };
            format!("{} {form}", words(c))
        }
    }
}

/// Number words without the currency noun.
fn words(n: u64) -> String {
    let billions = n / 1_000_000_000;
    let millions = (n / 1_000_000) % 1_000;
    let thousands = (n / 1_000) % 1_000;
    let rest = n % 1_000;
    let mut parts = Vec::new();
    if billions > 0 {
        parts.push(counted(
            billions,
            &BILLION,
            millions == 0 && thousands == 0 && rest == 0,
        ));
    }
    if millions > 0 {
        parts.push(counted(millions, &MILLION, thousands == 0 && rest == 0));
    }
    if thousands > 0 {
        parts.push(counted(thousands, &THOUSAND, rest == 0));
    }
    if rest > 0 {
        parts.push(below_1000(rest));
    }
    join(parts)
}

/// `n` followed by the currency noun, with agreement.
fn with_noun(n: u64, noun: &Noun) -> String {
    match n {
        0 => format!("صفر {}", noun.singular),
        1 => format!("{} واحد", noun.singular),
        2 => noun.dual.to_owned(),
        _ => {
            let form = match n % 100 {
                3..=10 => noun.plural,
                11..=99 => noun.accusative,
                _ => noun.singular,
            };
            format!("{} {form}", words(n))
        }
    }
}

pub(super) fn dinars(n: u64) -> String {
    with_noun(n, &DINAR)
}

pub(super) fn centimes(n: u64) -> String {
    with_noun(n, &CENTIME)
}
