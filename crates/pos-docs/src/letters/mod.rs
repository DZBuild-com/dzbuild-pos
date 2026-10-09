//! Amounts in letters (spec §6, DOC-004): "quatre mille cinq cents dinars et cinquante
//! centimes" / "أربعة آلاف وخمسمائة دينار وخمسون سنتيماً" / "four thousand five hundred dinars
//! and fifty centimes". In-house converters: the number grammar (French cent/vingt agreement,
//! Arabic dual, plural and accusative agreement, and the counted noun that agrees with the
//! number) is code, not translation. The words around it are document strings (spec §11) in
//! `pos-docs/locales`: `letters.negative` and `letters.dinarsAndCentimes`.

mod ar;
mod en;
mod fr;

use crate::locales;
use pos_core::lang::Lang;
use pos_core::money::Money;

/// The amount in words in `lang`: dinars, then centimes when there are any; zero is "zéro
/// dinar" / "صفر دينار" / "zero dinars"; a negative amount (refund) goes through
/// `letters.negative` ("moins …" / "ناقص …" / "minus …").
pub fn amount_in_letters(lang: Lang, amount: Money) -> String {
    let abs = amount.0.unsigned_abs();
    let (d, c) = (abs / 100, abs % 100);
    let body = match (d, c) {
        (_, 0) => dinars(lang, d),
        (0, _) => centimes(lang, c),
        _ => fill(
            lang,
            "letters.dinarsAndCentimes",
            "{{dinars}} {{centimes}}",
            &[
                ("dinars", &dinars(lang, d)),
                ("centimes", &centimes(lang, c)),
            ],
        ),
    };
    if amount.is_negative() {
        fill(
            lang,
            "letters.negative",
            "-{{amount}}",
            &[("amount", &body)],
        )
    } else {
        body
    }
}

fn dinars(lang: Lang, n: u64) -> String {
    match lang {
        Lang::Fr => fr::dinars(n),
        Lang::Ar => ar::dinars(n),
        Lang::En => en::dinars(n),
    }
}

fn centimes(lang: Lang, n: u64) -> String {
    match lang {
        Lang::Fr => fr::centimes(n),
        Lang::Ar => ar::centimes(n),
        Lang::En => en::centimes(n),
    }
}

/// The document string `key` in `lang` with each `{{name}}` replaced by its value. A catalogue
/// that lacks the key (the parity tests rule it out) falls back to `fallback`, which holds only
/// placeholders and signs, so the amount is never lost.
fn fill(lang: Lang, key: &str, fallback: &str, vars: &[(&str, &str)]) -> String {
    let template = locales::text(lang.as_str(), key).unwrap_or_else(|| fallback.to_owned());
    vars.iter().fold(template, |text, (name, value)| {
        text.replace(&format!("{{{{{name}}}}}"), value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The values spec §6 asks for, in dinars, then with centimes.
    const SPEC_VALUES: [i64; 12] = [
        0, 1, 2, 11, 21, 100, 101, 200, 1_000, 1_001, 2_000, 1_000_000,
    ];

    fn words(lang: Lang) -> Vec<String> {
        SPEC_VALUES
            .iter()
            .map(|d| amount_in_letters(lang, Money(d * 100)))
            .collect()
    }

    #[test]
    fn french_spec_values() {
        assert_eq!(
            words(Lang::Fr),
            [
                "zéro dinar",
                "un dinar",
                "deux dinars",
                "onze dinars",
                "vingt et un dinars",
                "cent dinars",
                "cent un dinars",
                "deux cents dinars",
                "mille dinars",
                "mille un dinars",
                "deux mille dinars",
                "un million de dinars",
            ]
        );
    }

    #[test]
    fn arabic_spec_values() {
        assert_eq!(
            words(Lang::Ar),
            [
                "صفر دينار",
                "دينار واحد",
                "ديناران",
                "أحد عشر ديناراً",
                "واحد وعشرون ديناراً",
                "مائة دينار",
                "مائة وواحد دينار",
                "مائتا دينار",
                "ألف دينار",
                "ألف وواحد دينار",
                "ألفا دينار",
                "مليون دينار",
            ]
        );
    }

    #[test]
    fn english_spec_values() {
        assert_eq!(
            words(Lang::En),
            [
                "zero dinars",
                "one dinar",
                "two dinars",
                "eleven dinars",
                "twenty-one dinars",
                "one hundred dinars",
                "one hundred one dinars",
                "two hundred dinars",
                "one thousand dinars",
                "one thousand one dinars",
                "two thousand dinars",
                "one million dinars",
            ]
        );
    }

    #[test]
    fn the_spec_example_with_centimes_in_three_languages() {
        let amount = Money(450_050);
        assert_eq!(
            amount_in_letters(Lang::Fr, amount),
            "quatre mille cinq cents dinars et cinquante centimes"
        );
        assert_eq!(
            amount_in_letters(Lang::Ar, amount),
            "أربعة آلاف وخمسمائة دينار وخمسون سنتيماً"
        );
        assert_eq!(
            amount_in_letters(Lang::En, amount),
            "four thousand five hundred dinars and fifty centimes"
        );
    }

    #[test]
    fn centimes_alone_and_negative_amounts() {
        assert_eq!(amount_in_letters(Lang::Fr, Money(1)), "un centime");
        assert_eq!(amount_in_letters(Lang::Ar, Money(1)), "سنتيم واحد");
        assert_eq!(amount_in_letters(Lang::Ar, Money(2)), "سنتيمان");
        assert_eq!(amount_in_letters(Lang::Ar, Money(5)), "خمسة سنتيمات");
        assert_eq!(amount_in_letters(Lang::Ar, Money(15)), "خمسة عشر سنتيماً");
        assert_eq!(amount_in_letters(Lang::En, Money(1)), "one centime");
        assert_eq!(
            amount_in_letters(Lang::Fr, Money(-150)),
            "moins un dinar et cinquante centimes"
        );
        assert_eq!(amount_in_letters(Lang::Ar, Money(-100)), "ناقص دينار واحد");
        assert_eq!(
            amount_in_letters(Lang::En, Money(-5_000)),
            "minus fifty dinars"
        );
    }

    #[test]
    fn french_agreement_of_cent_and_vingt() {
        let fr = |dinars: i64| amount_in_letters(Lang::Fr, Money(dinars * 100));
        assert_eq!(fr(80), "quatre-vingts dinars");
        assert_eq!(fr(81), "quatre-vingt-un dinars");
        assert_eq!(fr(71), "soixante et onze dinars");
        assert_eq!(fr(91), "quatre-vingt-onze dinars");
        assert_eq!(fr(80_000), "quatre-vingt mille dinars");
        assert_eq!(fr(200_000), "deux cent mille dinars");
        assert_eq!(fr(200_000_000), "deux cents millions de dinars");
        assert_eq!(fr(2_500_000), "deux millions cinq cent mille dinars");
        assert_eq!(fr(3_000_000_000), "trois milliards de dinars");
        assert_eq!(
            amount_in_letters(Lang::Fr, Money(29_999_950)),
            "deux cent quatre-vingt-dix-neuf mille neuf cent quatre-vingt-dix-neuf dinars et cinquante centimes"
        );
    }

    #[test]
    fn arabic_agreement_of_counted_nouns() {
        let ar = |dinars: i64| amount_in_letters(Lang::Ar, Money(dinars * 100));
        assert_eq!(ar(3), "ثلاثة دنانير");
        assert_eq!(ar(10), "عشرة دنانير");
        assert_eq!(ar(12), "اثنا عشر ديناراً");
        assert_eq!(ar(250), "مائتان وخمسون ديناراً");
        assert_eq!(ar(3_000), "ثلاثة آلاف دينار");
        assert_eq!(ar(11_000), "أحد عشر ألف دينار");
        assert_eq!(ar(11_500), "أحد عشر ألفاً وخمسمائة دينار");
        assert_eq!(ar(100_000), "مائة ألف دينار");
        assert_eq!(ar(200_500), "مائتا ألف وخمسمائة دينار");
        assert_eq!(ar(2_000_000), "مليونا دينار");
        assert_eq!(ar(2_500_000), "مليونان وخمسمائة ألف دينار");
        assert_eq!(ar(1_000_000_000), "مليار دينار");
    }

    #[test]
    fn the_largest_amount_does_not_panic() {
        for lang in Lang::ALL {
            assert!(!amount_in_letters(lang, Money(i64::MAX)).is_empty());
            assert!(!amount_in_letters(lang, Money(i64::MIN)).is_empty());
        }
    }

    /// Spec §11: the words around the number (the negative prefix, the conjunction between
    /// dinars and centimes) are document strings read from `pos-docs/locales` (`letters.*`),
    /// so a catalogue edit reaches the printed amount; only the number grammar is code.
    #[test]
    fn the_labels_around_the_number_come_from_the_document_catalogues() {
        for lang in Lang::ALL {
            let text = |key: &str| {
                crate::locales::text(lang.as_str(), key)
                    .unwrap_or_else(|| panic!("{lang:?} lacks {key}"))
            };
            let negative = text("letters.negative");
            let joined = text("letters.dinarsAndCentimes");
            assert!(negative.contains("{{amount}}"), "{lang:?} {negative}");
            assert!(
                joined.contains("{{dinars}}") && joined.contains("{{centimes}}"),
                "{lang:?} {joined}"
            );
            let dinars = amount_in_letters(lang, Money(100));
            let centimes = amount_in_letters(lang, Money(50));
            let both = joined
                .replace("{{dinars}}", &dinars)
                .replace("{{centimes}}", &centimes);
            assert_eq!(amount_in_letters(lang, Money(150)), both);
            assert_eq!(
                amount_in_letters(lang, Money(-150)),
                negative.replace("{{amount}}", &both)
            );
            assert_eq!(
                amount_in_letters(lang, Money(-50)),
                negative.replace("{{amount}}", &centimes)
            );
        }
    }
}
