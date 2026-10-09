//! The printed words of a label (`label.*`, and `test.labelTitle` of the label test page), from
//! the document catalogues `crates/pos-docs/locales/{ar,fr,en}.json`.

use pos_core::lang::Lang;

/// The word of `key` in `lang`, if the catalogue has it.
pub(crate) fn text(lang: Lang, key: &str) -> Option<String> {
    pos_docs::locales::text(lang.as_str(), key)
}

/// The word of `key` in `lang`; the key itself if the catalogue lacks it.
pub(crate) fn word(lang: Lang, key: &str) -> String {
    text(lang, key).unwrap_or_else(|| key.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_words_exist_in_the_three_languages() {
        assert_eq!(word(Lang::Fr, "label.expiry"), "À consommer avant le");
        assert_eq!(word(Lang::En, "label.packedOn"), "Packed on");
        assert_eq!(word(Lang::Ar, "label.weight"), "الوزن الصافي");
        assert_eq!(text(Lang::Ar, "label.units.kg").as_deref(), Some("كلغ"));
        assert_eq!(word(Lang::En, "test.labelTitle"), "Test label");
        assert_eq!(
            word(Lang::Fr, "doc.ticket"),
            "Ticket de caisse",
            "the document catalogues first"
        );
        assert_eq!(text(Lang::Fr, "label.units.pc"), None);
        assert_eq!(word(Lang::Fr, "label.nothing"), "label.nothing");
    }
}
