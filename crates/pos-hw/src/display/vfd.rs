//! 20×2 VFD pole displays (HW-043: Epson DM-D / ESC/POS and CD5220 command sets, research 05
//! §6.1). Fixed 5×7 fonts: Latin text and Western digits only. Arabic is never transliterated:
//! an Arabic ticket shows the French (or English) words, the product's Latin name, or "Article".

use super::frame::{DisplayFrame, DisplayLine, DisplayMode};
// `pos_docs::receipt::label` once the document catalogues carry the `display` words.
use super::words::word as label;
use crate::codepage::CodePage;
use crate::error::HwError;
use crate::fmt::{amount, qty};
use crate::serial::SerialSettings;
use pos_core::lang::Lang;
use pos_core::money::Money;
use serde::{Deserialize, Serialize};
use specta::Type;

pub const INIT: [u8; 2] = [0x1B, b'@'];
pub(crate) const CLEAR: u8 = 0x0C;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum VfdCommands {
    EscPos,
    Cd5220,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum VfdCharset {
    Ascii,
    Pc858,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VfdConfig {
    pub serial: SerialSettings,
    pub commands: VfdCommands,
    pub columns: u8,
    pub rows: u8,
    /// The words of an Arabic ticket's display: French or English.
    pub latin_lang: Lang,
    pub charset: VfdCharset,
    /// `ESC t n` for PC858.
    pub code_page_selector: u8,
}

impl VfdConfig {
    pub fn new(port: &str) -> VfdConfig {
        VfdConfig {
            serial: SerialSettings::eight_n1(port, 9_600),
            commands: VfdCommands::EscPos,
            columns: 20,
            rows: 2,
            latin_lang: Lang::Fr,
            charset: VfdCharset::Ascii,
            code_page_selector: 19,
        }
    }

    pub fn validate(&self) -> Result<(), HwError> {
        self.serial.validate()?;
        if !(16..=40).contains(&self.columns) {
            return Err(HwError::setting("columns"));
        }
        if !(1..=2).contains(&self.rows) {
            return Err(HwError::setting("rows"));
        }
        if self.latin_lang == Lang::Ar {
            return Err(HwError::setting("latinLang"));
        }
        Ok(())
    }
}

/// Accents off, ligatures spelled out, other scripts dropped: what a 5×7 ASCII font can show.
pub fn fold_ascii(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let folded: &str = match c {
            'à' | 'â' | 'ä' | 'á' => "a",
            'À' | 'Â' | 'Ä' | 'Á' => "A",
            'é' | 'è' | 'ê' | 'ë' => "e",
            'É' | 'È' | 'Ê' | 'Ë' => "E",
            'î' | 'ï' | 'í' => "i",
            'Î' | 'Ï' | 'Í' => "I",
            'ô' | 'ö' | 'ó' => "o",
            'Ô' | 'Ö' | 'Ó' => "O",
            'ù' | 'û' | 'ü' | 'ú' => "u",
            'Ù' | 'Û' | 'Ü' | 'Ú' => "U",
            'ç' => "c",
            'Ç' => "C",
            'œ' => "oe",
            'Œ' => "OE",
            'æ' => "ae",
            'Æ' => "AE",
            '×' => "x",
            '–' | '—' | '−' => "-",
            '’' | '‘' => "'",
            '\u{A0}' | '\u{202F}' | '\t' => " ",
            c if (' '..='~').contains(&c) => {
                out.push(c);
                continue;
            }
            _ => continue,
        };
        out.push_str(folded);
    }
    out
}

/// What the display's font can show, one byte a column: in PC858 a character the page lacks, or
/// prints on several bytes (`œ`) or none (a bidi mark), goes through the ASCII fold instead.
fn clean(text: &str, cfg: &VfdConfig) -> String {
    match cfg.charset {
        VfdCharset::Ascii => fold_ascii(text),
        VfdCharset::Pc858 => text
            .chars()
            .map(|c| {
                let one = if c.is_whitespace() {
                    " ".to_owned()
                } else {
                    c.to_string()
                };
                let page = CodePage::Pc858;
                if page.can_encode(&one) && page.encode(&one).len() == 1 {
                    one
                } else {
                    fold_ascii(&one)
                }
            })
            .collect(),
    }
}

fn fit(text: &str, width: usize) -> String {
    let cut: String = text.chars().take(width).collect();
    format!("{cut:<width$}")
}

fn center(text: &str, width: usize) -> String {
    let cut: String = text.trim().chars().take(width).collect();
    let pad = width - cut.chars().count();
    fit(&format!("{}{cut}", " ".repeat(pad / 2)), width)
}

/// `start` cut to leave room for `end`, which is kept whole at the end of the line.
fn pair(start: &str, end: &str, width: usize) -> String {
    let end: String = end.chars().take(width).collect();
    let room = width.saturating_sub(end.chars().count() + 1);
    let start: String = start.trim().chars().take(room).collect();
    let gap = width - start.chars().count() - end.chars().count();
    format!("{start}{}{end}", " ".repeat(gap))
}

pub fn lines(frame: &DisplayFrame, shop: &str, cfg: &VfdConfig) -> Vec<String> {
    let lang = if frame.lang == Lang::Ar {
        cfg.latin_lang
    } else {
        frame.lang
    };
    let w = usize::from(cfg.columns);
    let word = |key: &str| clean(&label(lang, &format!("display.{key}")), cfg);
    let money = |m: Money| clean(&amount(m, lang), cfg);
    let latin = |line: &DisplayLine| {
        let name = clean(&line.latin_name, cfg);
        if name.trim().is_empty() {
            word("item")
        } else {
            name
        }
    };
    let (top, bottom) = match frame.mode {
        DisplayMode::Idle => (center(&clean(shop, cfg), w), center(&word("welcome"), w)),
        DisplayMode::Paid => (
            pair(&word("total"), &money(frame.total), w),
            match frame.change {
                Some(change) if change.is_positive() => pair(&word("change"), &money(change), w),
                _ => center(&word("thanks"), w),
            },
        ),
        DisplayMode::Sale => match (&frame.weight, &frame.line) {
            (Some(weight), line) => {
                let name = line.as_ref().map_or_else(|| word("item"), latin);
                let per_kg = weight
                    .unit_price
                    .map(|p| format!("{}/kg", money(p)))
                    .unwrap_or_default();
                (
                    fit(&name, w),
                    pair(
                        &format!("{} kg", clean(&qty(weight.net, lang), cfg)),
                        &per_kg,
                        w,
                    ),
                )
            }
            (None, Some(line)) => (
                pair(&latin(line), &money(line.total), w),
                pair(&word("total"), &money(frame.total), w),
            ),
            (None, None) => (
                pair(&word("items"), &clean(&qty(frame.items, lang), cfg), w),
                pair(&word("total"), &money(frame.total), w),
            ),
        },
    };
    if cfg.rows == 1 {
        vec![bottom_or_total(frame, top, bottom)]
    } else {
        vec![top, bottom]
    }
}

/// A one-line display shows the amount: the total line (the first line of a paid screen).
fn bottom_or_total(frame: &DisplayFrame, top: String, bottom: String) -> String {
    if frame.mode == DisplayMode::Paid {
        top
    } else {
        bottom
    }
}

pub fn bytes(lines: &[String], cfg: &VfdConfig) -> Vec<u8> {
    let mut out = vec![CLEAR];
    if cfg.charset == VfdCharset::Pc858 {
        out.extend([0x1B, b't', cfg.code_page_selector]);
    }
    let encode = |s: &str| -> Vec<u8> {
        match cfg.charset {
            VfdCharset::Ascii => s.bytes().filter(|b| (0x20..0x7F).contains(b)).collect(),
            VfdCharset::Pc858 => CodePage::Pc858.encode(s),
        }
    };
    for (i, line) in lines.iter().enumerate() {
        match cfg.commands {
            VfdCommands::EscPos => {
                out.extend([0x1F, b'$', 1, u8::try_from(i + 1).unwrap_or(1)]);
                out.extend(encode(line));
            }
            VfdCommands::Cd5220 => {
                out.extend([0x1B, b'Q', if i == 0 { b'A' } else { b'B' }]);
                out.extend(encode(line));
                out.push(0x0D);
            }
        }
    }
    out
}

/// "DZBuild POS" and the digits: the settings' "Tester l'afficheur".
pub fn test_lines(cfg: &VfdConfig) -> Vec<String> {
    let w = usize::from(cfg.columns);
    let all = vec![center("DZBuild POS", w), center("0123456789 ,.-/", w)];
    all.into_iter().take(usize::from(cfg.rows)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::frame::{DisplayLine, DisplayMode, DisplayWeight};
    use pos_core::money::{Money, Qty};
    use proptest::prelude::*;

    fn cfg() -> VfdConfig {
        VfdConfig::new("COM2")
    }

    fn sale(latin: &str, total_line: i64, total: i64, lang: Lang) -> DisplayFrame {
        DisplayFrame {
            mode: DisplayMode::Sale,
            line: Some(DisplayLine {
                name: latin.into(),
                latin_name: latin.into(),
                qty: Qty(3_000),
                unit_code: "pc".into(),
                unit_price: Money(15_000),
                total: Money(total_line),
            }),
            items: Qty(5_000),
            total: Money(total),
            ..DisplayFrame::idle(lang)
        }
    }

    #[test]
    fn vfd_lines_never_exceed_the_columns_and_fold_accents() {
        let lines = lines(
            &sale("Crème fraîche épaisse 20 cl", 45_000, 125_000, Lang::Fr),
            "Supérette",
            &cfg(),
        );
        assert_eq!(lines.len(), 2);
        assert!(
            lines
                .iter()
                .all(|l| l.chars().count() == 20 && l.is_ascii()),
            "{lines:?}"
        );
        assert!(
            lines[0].starts_with("Creme fraic") && lines[0].ends_with("450,00"),
            "{:?}",
            lines[0]
        );
        assert_eq!(lines[1], "Total       1 250,00");
    }

    #[test]
    fn idle_weight_and_paid_screens() {
        let c = cfg();
        assert_eq!(
            lines(&DisplayFrame::idle(Lang::Fr), "Supérette El Amel", &c),
            [" Superette El Amel  ", "     Bienvenue      "]
        );
        let mut weighing = sale("Tomates", 0, 0, Lang::Fr);
        weighing.weight = Some(DisplayWeight {
            net: Qty(1_250),
            stable: true,
            unit_price: Some(Money(12_000)),
            tare: Qty::ZERO,
        });
        assert_eq!(lines(&weighing, "", &c)[1], "1,25 kg    120,00/kg");
        let paid = DisplayFrame {
            mode: DisplayMode::Paid,
            total: Money(125_000),
            paid: Some(Money(130_000)),
            change: Some(Money(5_000)),
            ..DisplayFrame::idle(Lang::Fr)
        };
        assert_eq!(
            lines(&paid, "", &c),
            ["Total       1 250,00", "Rendu          50,00"]
        );
        let exact = DisplayFrame {
            change: Some(Money::ZERO),
            ..paid
        };
        assert_eq!(lines(&exact, "", &c)[1], "      Merci !       ");
        let one_row = VfdConfig { rows: 1, ..c };
        assert_eq!(lines(&exact, "", &one_row), ["Total       1 250,00"]);
    }

    #[test]
    fn arabic_frames_speak_the_latin_language_and_names_fall_back() {
        let c = cfg();
        let mut frame = sale("", 30_000, 30_000, Lang::Ar);
        if let Some(line) = frame.line.as_mut() {
            line.name = "حليب كانديا".into();
        }
        let shown = lines(&frame, "سوبيرات الأمل", &c);
        assert!(
            shown[0].starts_with("Article") && shown[0].ends_with("300,00"),
            "{shown:?}"
        );
        assert!(shown[1].starts_with("Total"));
        let english = VfdConfig {
            latin_lang: Lang::En,
            ..c
        };
        assert!(lines(&frame, "", &english)[0].starts_with("Item"));
        assert_eq!(
            fold_ascii("Œufs × 30 — prix spécial"),
            "OEufs x 30 - prix special"
        );
        assert_eq!(fold_ascii("حليب 1L"), " 1L");
    }

    #[test]
    fn escpos_and_cd5220_bytes() {
        let shown = vec!["AB".to_owned(), "CD".to_owned()];
        assert_eq!(
            bytes(&shown, &cfg()),
            [
                0x0C, 0x1F, b'$', 1, 1, b'A', b'B', 0x1F, b'$', 1, 2, b'C', b'D'
            ]
        );
        let cd = VfdConfig {
            commands: VfdCommands::Cd5220,
            ..cfg()
        };
        assert_eq!(
            bytes(&shown, &cd),
            [
                0x0C, 0x1B, b'Q', b'A', b'A', b'B', 0x0D, 0x1B, b'Q', b'B', b'C', b'D', 0x0D
            ]
        );
        let latin = VfdConfig {
            charset: VfdCharset::Pc858,
            ..cfg()
        };
        let accented = bytes(&["é".to_owned()], &latin);
        assert_eq!(&accented[..4], &[0x0C, 0x1B, b't', 19]);
        assert!(accented.contains(&0x82), "é in PC858");
    }

    #[test]
    fn a_pc858_display_keeps_its_accents_on_one_byte_a_column() {
        let latin = VfdConfig {
            charset: VfdCharset::Pc858,
            ..cfg()
        };
        let shown = lines(
            &sale("Œuf\u{200F} crème", 45_000, 125_000, Lang::Fr),
            "",
            &latin,
        );
        assert_eq!(shown[0], "OEuf crème    450,00");
        let sent = bytes(&shown, &latin);
        assert_eq!(
            sent.len(),
            4 + 2 * (4 + 20),
            "one byte a column: {sent:02X?}"
        );
        assert!(sent.contains(&0x8A), "è in PC858");
    }

    #[test]
    fn configs_are_validated() {
        let field = |c: VfdConfig| c.validate().unwrap_err().field().map(str::to_owned);
        assert_eq!(
            field(VfdConfig {
                columns: 8,
                ..cfg()
            })
            .as_deref(),
            Some("columns")
        );
        assert_eq!(
            field(VfdConfig { rows: 3, ..cfg() }).as_deref(),
            Some("rows")
        );
        assert_eq!(
            field(VfdConfig {
                latin_lang: Lang::Ar,
                ..cfg()
            })
            .as_deref(),
            Some("latinLang")
        );
    }

    proptest! {
        #[test]
        fn any_name_and_amount_fit_the_display(name in "\\PC{0,60}", total in 0_i64..100_000_000_000) {
            let shown = lines(&sale(&name, total, total, Lang::Fr), &name, &cfg());
            prop_assert!(shown.iter().all(|l| l.chars().count() == 20 && l.is_ascii()));
        }

        #[test]
        fn any_name_fits_a_pc858_display_one_byte_a_column(name in "\\PC{0,60}", total in 0_i64..100_000_000_000) {
            let latin = VfdConfig { charset: VfdCharset::Pc858, ..cfg() };
            let shown = lines(&sale(&name, total, total, Lang::Fr), &name, &latin);
            prop_assert!(shown.iter().all(|l| l.chars().count() == 20));
            prop_assert_eq!(bytes(&shown, &latin).len(), 4 + 2 * (4 + 20));
        }
    }
}
