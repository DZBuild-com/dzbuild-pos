//! The pole display's words (`display.*`). Their home is the document catalogues
//! `crates/pos-docs/locales/{ar,fr,en}.json`, read first (as `pos_docs::receipt::label` does);
//! until those carry the `display` object, the words below stand in (M4 hand-off: move them
//! there, then delete this table). The Arabic words exist for parity and for the second screen's
//! future text layer: the VFD never shows them.

use pos_core::lang::Lang;

/// Key, then the word in ar, fr and en.
const WORDS: [(&str, [&str; 3]); 6] = [
    ("display.welcome", ["مرحباً", "Bienvenue", "Welcome"]),
    ("display.total", ["المجموع", "Total", "Total"]),
    ("display.change", ["الباقي", "Rendu", "Change"]),
    ("display.thanks", ["شكراً!", "Merci !", "Thank you!"]),
    ("display.items", ["السلع", "Articles", "Items"]),
    ("display.item", ["سلعة", "Article", "Item"]),
];

/// The word of `key` in `lang`; the key itself if no catalogue has it.
pub(crate) fn word(lang: Lang, key: &str) -> String {
    pos_docs::locales::text(lang.as_str(), key)
        .or_else(|| {
            let column = match lang {
                Lang::Ar => 0,
                Lang::Fr => 1,
                Lang::En => 2,
            };
            WORDS
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, words)| words[column].to_owned())
        })
        .unwrap_or_else(|| key.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_words_exist_in_the_three_languages() {
        assert_eq!(word(Lang::Fr, "display.welcome"), "Bienvenue");
        assert_eq!(word(Lang::En, "display.change"), "Change");
        assert_eq!(word(Lang::Ar, "display.thanks"), "شكراً!");
        assert_eq!(word(Lang::Fr, "display.item"), "Article");
        assert_eq!(
            word(Lang::Fr, "doc.ticket"),
            "Ticket de caisse",
            "the document catalogues first"
        );
        assert_eq!(word(Lang::Fr, "display.nothing"), "display.nothing");
    }
}
