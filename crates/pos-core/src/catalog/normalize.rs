//! Search normalisation (spec §8): the same folding is applied to what is indexed
//! (`products.search_text`) and to what the cashier types, so that "Crème", "creme" and "CRÈME"
//! meet, and so do "أحمد", "احمد" and "أَحْمَد".
//!
//! - Unicode compatibility decomposition (NFKD), then every combining mark is dropped: Latin
//!   accents, Arabic tashkeel, and the hamza/madda of أ إ آ ؤ ئ (which leaves ا ا ا و ي).
//! - Arabic: tatweel removed, ٱ → ا, ى → ي, ة → ه (Persian ی ک → ي ك).
//! - Latin: lower case; œ → oe, æ → ae, ß → ss.
//! - Digits: Arabic-Indic and Persian digits become 0-9 (full-width ones already do under NFKD).
//! - Anything that is not a letter or a digit separates words.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

const TATWEEL: char = '\u{0640}';
const ARABIC_ARTICLE: &str = "\u{0627}\u{0644}";

fn fold_char(c: char, out: &mut String) {
    match c {
        TATWEEL => {}
        '\u{0671}' => out.push('\u{0627}'),
        '\u{0649}' | '\u{06CC}' => out.push('\u{064A}'),
        '\u{0629}' => out.push('\u{0647}'),
        '\u{06A9}' => out.push('\u{0643}'),
        '\u{0660}'..='\u{0669}' => out.push(shift_digit(c, 0x0660)),
        '\u{06F0}'..='\u{06F9}' => out.push(shift_digit(c, 0x06F0)),
        'œ' | 'Œ' => out.push_str("oe"),
        'æ' | 'Æ' => out.push_str("ae"),
        'ß' => out.push_str("ss"),
        c if c.is_alphanumeric() => {
            for lower in c.to_lowercase() {
                if !is_combining_mark(lower) {
                    out.push(lower);
                }
            }
        }
        _ => out.push(' '),
    }
}

/// `c` is in a 10-digit block starting at `zero`: the Western digit of the same value.
fn shift_digit(c: char, zero: u32) -> char {
    char::from_digit(u32::from(c).saturating_sub(zero), 10).unwrap_or('0')
}

/// The normalised words of `text`, in order.
pub fn words(text: &str) -> Vec<String> {
    let mut folded = String::with_capacity(text.len());
    for c in text.nfkd() {
        if !is_combining_mark(c) {
            fold_char(c, &mut folded);
        }
    }
    folded.split_whitespace().map(str::to_owned).collect()
}

/// `text` normalised, words joined by one space.
pub fn normalize(text: &str) -> String {
    words(text).join(" ")
}

/// The text stored in `products.search_text`: every word of every part (names in the three
/// languages, reference, barcodes), plus each Arabic word that starts with the article "ال"
/// once more without it, so that "حليب" finds "الحليب".
pub fn search_text<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    let mut out: Vec<String> = Vec::new();
    for part in parts {
        for word in words(part) {
            let bare = word
                .strip_prefix(ARABIC_ARTICLE)
                .filter(|rest| rest.chars().count() >= 2)
                .map(str::to_owned);
            out.push(word);
            out.extend(bare);
        }
    }
    out.join(" ")
}

/// The FTS5 `MATCH` expression for what the cashier typed: every word as a quoted prefix,
/// all required. `None` when nothing searchable was typed. Quotes cannot occur inside a word
/// (they separate words), so the expression cannot be broken by the input.
pub fn fts_query(query: &str) -> Option<String> {
    let words = words(query);
    if words.is_empty() {
        return None;
    }
    Some(
        words
            .iter()
            .map(|w| format!("\"{w}\"*"))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arabic_is_folded_like_the_spec_says() {
        assert_eq!(normalize("أَحْمَد"), "احمد", "tashkeel and hamza on alef");
        assert_eq!(normalize("إبريق"), "ابريق");
        assert_eq!(normalize("آلة"), "اله", "madda, and ta marbuta -> ha");
        assert_eq!(normalize("مؤسسة"), "موسسه", "hamza on waw");
        assert_eq!(normalize("شاطئ"), "شاطي", "hamza on ya");
        assert_eq!(normalize("مصطفى"), "مصطفي", "alef maqsura");
        assert_eq!(normalize("حـــليب"), "حليب", "tatweel");
        assert_eq!(normalize("ٱلقهوة"), "القهوه", "alef wasla");
        assert_eq!(normalize("ﻻ"), "لا", "presentation-form ligature");
    }

    #[test]
    fn digits_become_western() {
        assert_eq!(normalize("قهوة ١٢٣"), "قهوه 123");
        assert_eq!(normalize("۱۲"), "12");
        assert_eq!(normalize("１２３"), "123");
    }

    #[test]
    fn latin_is_case_folded_without_accents() {
        assert_eq!(normalize("Crème Brûlée"), "creme brulee");
        assert_eq!(normalize("ŒUF Élevé"), "oeuf eleve");
        assert_eq!(normalize("Café-Noir 250g"), "cafe noir 250g");
        assert_eq!(normalize("ÇA"), "ca");
        assert_eq!(normalize("ﬁlet"), "filet");
        assert_eq!(normalize("  x  "), "x");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn search_text_adds_the_arabic_word_without_its_article() {
        assert_eq!(
            search_text(["الحليب", "Lait", "Milk", "613000"]),
            "الحليب حليب lait milk 613000"
        );
        assert_eq!(search_text(["الم"]), "الم", "too short to strip");
    }

    #[test]
    fn queries_are_quoted_prefixes_and_cannot_inject_syntax() {
        assert_eq!(fts_query("Lait dem"), Some("\"lait\"* \"dem\"*".into()));
        assert_eq!(
            fts_query("\"x\" OR y*"),
            Some("\"x\"* \"or\"* \"y\"*".into())
        );
        assert_eq!(fts_query(" \"*( "), None);
        assert_eq!(fts_query("حليب"), Some("\"حليب\"*".into()));
    }
}
