//! Latin code pages for text-mode receipts (spec §9.1: CP858 or WPC1252). Arabic never goes
//! through a code page: a receipt with any character the page lacks is printed as a raster.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CodePage {
    /// PC858: PC850 with the euro sign at 0xD5.
    Pc858,
    /// Windows-1252.
    Wpc1252,
}

/// Bytes 0x80..=0xFF of PC858.
const PC858_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', //
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', 'ø', '£', 'Ø', '×', 'ƒ', //
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '®', '¬', '½', '¼', '¡', '«', '»', //
    '░', '▒', '▓', '│', '┤', 'Á', 'Â', 'À', '©', '╣', '║', '╗', '╝', '¢', '¥', '┐', //
    '└', '┴', '┬', '├', '─', '┼', 'ã', 'Ã', '╚', '╔', '╩', '╦', '╠', '═', '╬', '¤', //
    'ð', 'Ð', 'Ê', 'Ë', 'È', '€', 'Í', 'Î', 'Ï', '┘', '┌', '█', '▄', '¦', 'Ì', '▀', //
    'Ó', 'ß', 'Ô', 'Ò', 'õ', 'Õ', 'µ', 'þ', 'Þ', 'Ú', 'Û', 'Ù', 'ý', 'Ý', '¯', '´', //
    '\u{AD}', '±', '‗', '¾', '¶', '§', '÷', '¸', '°', '¨', '·', '¹', '³', '²', '■', '\u{A0}',
];

/// Bytes 0x80..=0x9F of Windows-1252 (0xA0..=0xFF equal U+00A0..=U+00FF).
#[rustfmt::skip]
const WPC1252_80: [Option<char>; 32] = [
    Some('€'), None,      Some('‚'), Some('ƒ'), Some('„'), Some('…'), Some('†'), Some('‡'),
    Some('ˆ'), Some('‰'), Some('Š'), Some('‹'), Some('Œ'), None,      Some('Ž'), None,
    None,      Some('‘'), Some('’'), Some('“'), Some('”'), Some('•'), Some('–'), Some('—'),
    Some('˜'), Some('™'), Some('š'), Some('›'), Some('œ'), None,      Some('ž'), Some('Ÿ'),
];

/// Spaces of every width print as one space; bidi marks print as nothing.
fn plain(c: char) -> Option<char> {
    match c {
        '\u{00A0}' | '\u{202F}' | '\u{2009}' | '\u{2007}' => Some(' '),
        '\u{200E}' | '\u{200F}' | '\u{2066}'..='\u{2069}' | '\u{202A}'..='\u{202E}' => None,
        other => Some(other),
    }
}

/// What a cashier would type for a character the page lacks.
fn fold(c: char) -> Option<&'static str> {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{2032}' => Some("'"),
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{2033}' => Some("\""),
        '\u{2013}' | '\u{2014}' | '\u{2212}' => Some("-"),
        '\u{2026}' => Some("..."),
        '\u{0152}' => Some("OE"),
        '\u{0153}' => Some("oe"),
        _ => None,
    }
}

impl CodePage {
    /// `ESC t n` for this page on Epson-compatible printers (Xprinter, Rongta, HPRT, Sunmi).
    pub const fn default_selector(self) -> u8 {
        match self {
            CodePage::Pc858 => 19,
            CodePage::Wpc1252 => 16,
        }
    }

    fn byte(self, c: char) -> Option<u8> {
        let code = u32::from(c);
        if code < 0x80 {
            // C0 controls and DEL are commands or nothing on a printer, never text
            return u8::try_from(code).ok().filter(|b| (0x20..0x7F).contains(b));
        }
        let high = |index: usize| u8::try_from(0x80 + index).ok();
        match self {
            CodePage::Pc858 => PC858_HIGH.iter().position(|&h| h == c).and_then(high),
            CodePage::Wpc1252 if (0xA0..=0xFF).contains(&code) => u8::try_from(code).ok(),
            CodePage::Wpc1252 => WPC1252_80.iter().position(|&h| h == Some(c)).and_then(high),
        }
    }

    /// Whether every character prints in text mode (after the folds above).
    pub fn can_encode(self, text: &str) -> bool {
        text.chars()
            .filter_map(plain)
            .all(|c| self.byte(c).is_some() || fold(c).is_some())
    }

    /// The bytes of `text`; a character the page lacks even after folding becomes `?`.
    pub fn encode(self, text: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(text.len());
        for c in text.chars().filter_map(plain) {
            if let Some(b) = self.byte(c) {
                out.push(b);
            } else if let Some(folded) = fold(c) {
                out.extend(folded.chars().filter_map(|f| self.byte(f)));
            } else {
                out.push(b'?');
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A byte back to its character through the tables above (the test's decoder).
    fn decode_byte(page: CodePage, byte: u8) -> Option<char> {
        match (page, byte) {
            (_, 0..=0x7F) => Some(char::from(byte)),
            (CodePage::Pc858, _) => PC858_HIGH.get(usize::from(byte - 0x80)).copied(),
            (CodePage::Wpc1252, 0x80..=0x9F) => {
                WPC1252_80.get(usize::from(byte - 0x80)).copied().flatten()
            }
            (CodePage::Wpc1252, _) => Some(char::from(byte)),
        }
    }

    fn decode(page: CodePage, bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|&b| decode_byte(page, b).unwrap_or('\u{FFFD}'))
            .collect()
    }

    #[test]
    fn ascii_is_unchanged() {
        assert_eq!(CodePage::Pc858.encode("TK2026-C1 x2"), b"TK2026-C1 x2");
        assert!(CodePage::Wpc1252.can_encode("Total TTC: 4,500.00"));
    }

    #[test]
    fn french_accents_in_both_pages() {
        assert_eq!(
            CodePage::Pc858.encode("Épicerie à côté"),
            b"\x90picerie \x85 c\x93t\x82"
        );
        assert_eq!(
            CodePage::Wpc1252.encode("Épicerie à côté"),
            b"\xC9picerie \xE0 c\xF4t\xE9"
        );
        assert_eq!(CodePage::Pc858.encode("€ × °"), b"\xD5 \x9E \xF8");
        assert_eq!(CodePage::Wpc1252.encode("€ œ"), b"\x80 \x9C");
    }

    /// Expected bytes from Windows' own tables (`Encoding.GetEncoding(858)` and `(1252)`).
    #[test]
    fn french_set_and_currency_round_trip() {
        const FRENCH: &str = "àâäçéèêëîïôöùûüÿÀÂÄÇÉÈÊËÎÏÔÖÙÛÜæÆ€°×«»";
        assert_eq!(
            CodePage::Pc858.encode(FRENCH),
            [
                0x85, 0x83, 0x84, 0x87, 0x82, 0x8A, 0x88, 0x89, 0x8C, 0x8B, 0x93, 0x94, 0x97, 0x96,
                0x81, 0x98, 0xB7, 0xB6, 0x8E, 0x80, 0x90, 0xD4, 0xD2, 0xD3, 0xD7, 0xD8, 0xE2, 0x99,
                0xEB, 0xEA, 0x9A, 0x91, 0x92, 0xD5, 0xF8, 0x9E, 0xAE, 0xAF,
            ]
        );
        assert_eq!(
            CodePage::Wpc1252.encode(FRENCH),
            [
                0xE0, 0xE2, 0xE4, 0xE7, 0xE9, 0xE8, 0xEA, 0xEB, 0xEE, 0xEF, 0xF4, 0xF6, 0xF9, 0xFB,
                0xFC, 0xFF, 0xC0, 0xC2, 0xC4, 0xC7, 0xC9, 0xC8, 0xCA, 0xCB, 0xCE, 0xCF, 0xD4, 0xD6,
                0xD9, 0xDB, 0xDC, 0xE6, 0xC6, 0x80, 0xB0, 0xD7, 0xAB, 0xBB,
            ]
        );
        assert_eq!(CodePage::Wpc1252.encode("œŒŸ"), [0x9C, 0x8C, 0x9F]);
        for page in [CodePage::Pc858, CodePage::Wpc1252] {
            for text in [FRENCH, "Total TTC 4 500,00 DA", "Prix : 12,50 €"] {
                assert!(page.can_encode(text), "{page:?} {text}");
                assert_eq!(decode(page, &page.encode(text)), text, "{page:?}");
            }
            assert!(
                !page.can_encode("4 500,00 دج"),
                "the Arabic dinar sign goes to raster"
            );
        }
    }

    #[test]
    fn every_high_byte_round_trips() {
        for page in [CodePage::Pc858, CodePage::Wpc1252] {
            for byte in 0x80..=0xFF_u8 {
                // a no-break space prints as a plain space
                if let Some(c) = decode_byte(page, byte)
                    && c != '\u{A0}'
                {
                    assert_eq!(page.encode(&c.to_string()), [byte], "{page:?} {byte:#04X}");
                }
            }
        }
    }

    #[test]
    fn typographic_characters_are_folded_not_lost() {
        assert_eq!(
            CodePage::Pc858.encode("l’œuf – «bio»…"),
            b"l'oeuf - \xAEbio\xAF..."
        );
        assert_eq!(
            CodePage::Pc858.encode("4\u{202F}500,00\u{00A0}DA"),
            b"4 500,00 DA"
        );
        assert!(CodePage::Pc858.can_encode("l’œuf – «bio»…"));
    }

    #[test]
    fn arabic_cannot_be_encoded() {
        assert!(!CodePage::Pc858.can_encode("حليب"));
        assert!(!CodePage::Wpc1252.can_encode("Lait حليب"));
        assert_eq!(CodePage::Pc858.encode("حليب"), b"????");
    }

    #[test]
    fn delete_is_a_control_character() {
        for page in [CodePage::Pc858, CodePage::Wpc1252] {
            assert!(!page.can_encode("Lait\u{7F}"), "{page:?}");
            assert_eq!(page.encode("a\u{7F}b"), b"a?b", "{page:?}");
            assert!(page.can_encode("~"), "{page:?}: 0x7E still prints");
        }
    }

    #[test]
    fn selectors_are_the_epson_ones() {
        assert_eq!(CodePage::Pc858.default_selector(), 19);
        assert_eq!(CodePage::Wpc1252.default_selector(), 16);
    }
}
