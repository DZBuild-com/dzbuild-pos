//! Printer profiles (HW-006): paper, dots, code page, cut, drawer pin, raster bands, and presets
//! for the models sold in Algeria.

pub use crate::codepage::CodePage;
use crate::error::HwError;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::ops::RangeInclusive;

/// Rows a `GS v 0` band may have: small enough for the buffer of a cheap printer, large enough
/// not to flood the port with headers.
pub const BAND_ROWS: RangeInclusive<u16> = 8..=1024;

/// Width in dots of a Font A character, the font text mode prints (`ESC @` selects it).
pub const FONT_A_DOTS: u16 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PaperWidth {
    Mm58,
    Mm80,
}

/// Text mode for Latin receipts, raster for Arabic ones (`Auto`); `Text` still switches to raster
/// when a character cannot print, `Raster` always rasterises (HW-005 "Texte" / "Image (arabe)").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RenderMode {
    Auto,
    Text,
    Raster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CutMode {
    Full,
    Partial,
    None,
}

/// The drawer connector pin the kick pulses (HW-040: pin 2, pin 5 as the fallback).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DrawerPin {
    Pin2,
    Pin5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    Generic,
    Xprinter,
    Epson,
    Rongta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub preset: Preset,
    pub width: PaperWidth,
    /// Printable dots per line: 384 on 58 mm, 576 on 80 mm (some 80 mm heads have 512).
    pub dots: u16,
    /// Font A characters per line in text mode: 32 on 58 mm, 48 on 80 mm (42 on 512 dots).
    pub columns: u8,
    pub render: RenderMode,
    pub code_page: CodePage,
    /// `ESC t n` for `code_page` on this model.
    pub code_page_selector: u8,
    pub cut: CutMode,
    /// Lines fed before the cut so the last line clears the blade.
    pub feed_before_cut: u8,
    pub drawer_pin: DrawerPin,
    /// Rows per `GS v 0` band.
    pub band_rows: u16,
    /// Send `FS .` first (Chinese firmware, HW-006).
    pub exit_chinese_mode: bool,
}

impl Profile {
    pub fn preset(preset: Preset, width: PaperWidth) -> Profile {
        let (dots, columns) = match width {
            PaperWidth::Mm58 => (384, 32),
            PaperWidth::Mm80 => (576, 48),
        };
        let base = Profile {
            preset,
            width,
            dots,
            columns,
            render: RenderMode::Auto,
            code_page: CodePage::Pc858,
            code_page_selector: CodePage::Pc858.default_selector(),
            cut: CutMode::Partial,
            feed_before_cut: 4,
            drawer_pin: DrawerPin::Pin2,
            band_rows: 128,
            exit_chinese_mode: false,
        };
        match preset {
            Preset::Generic => base,
            Preset::Xprinter => Profile {
                exit_chinese_mode: true,
                band_rows: 96,
                ..base
            },
            Preset::Epson => Profile {
                feed_before_cut: 3,
                ..base
            },
            Preset::Rongta => Profile {
                exit_chinese_mode: true,
                band_rows: 64,
                ..base
            },
        }
    }

    pub fn validate(&self) -> Result<(), HwError> {
        if !(256..=832).contains(&self.dots) || !self.dots.is_multiple_of(8) {
            return Err(HwError::invalid("profile.dots"));
        }
        // a line of `columns` Font A characters must fit the head, or the printer wraps it
        if !(24..=64).contains(&self.columns) || u16::from(self.columns) * FONT_A_DOTS > self.dots {
            return Err(HwError::invalid("profile.columns"));
        }
        if !BAND_ROWS.contains(&self.band_rows) {
            return Err(HwError::invalid("profile.bandRows"));
        }
        if self.feed_before_cut > 10 {
            return Err(HwError::invalid("profile.feedBeforeCut"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_match_the_paper() {
        let p58 = Profile::preset(Preset::Generic, PaperWidth::Mm58);
        assert_eq!((p58.dots, p58.columns), (384, 32));
        let p80 = Profile::preset(Preset::Xprinter, PaperWidth::Mm80);
        assert_eq!((p80.dots, p80.columns), (576, 48));
        assert!(
            p80.exit_chinese_mode,
            "Xprinter firmware boots in Chinese mode (HW-006)"
        );
        for preset in [
            Preset::Generic,
            Preset::Xprinter,
            Preset::Epson,
            Preset::Rongta,
        ] {
            for width in [PaperWidth::Mm58, PaperWidth::Mm80] {
                Profile::preset(preset, width).validate().unwrap();
            }
        }
    }

    #[test]
    fn invalid_profiles_name_their_field() {
        let base = Profile::preset(Preset::Generic, PaperWidth::Mm80);
        let field = |p: Profile| p.validate().unwrap_err().field().map(str::to_owned);
        assert_eq!(
            field(Profile {
                dots: 570,
                ..base.clone()
            })
            .as_deref(),
            Some("profile.dots")
        );
        assert_eq!(
            field(Profile {
                columns: 8,
                ..base.clone()
            })
            .as_deref(),
            Some("profile.columns")
        );
        assert_eq!(
            field(Profile {
                band_rows: 0,
                ..base.clone()
            })
            .as_deref(),
            Some("profile.bandRows")
        );
        assert_eq!(
            field(Profile {
                feed_before_cut: 40,
                ..base
            })
            .as_deref(),
            Some("profile.feedBeforeCut")
        );
    }

    /// Text mode prints Font A, 12 dots a character: more columns than fit would wrap every full
    /// line on the printer itself.
    #[test]
    fn columns_fit_the_head_in_font_a() {
        let p80 = Profile::preset(Preset::Generic, PaperWidth::Mm80);
        let p58 = Profile::preset(Preset::Generic, PaperWidth::Mm58);
        for (dots, columns, ok) in [
            (576, 48, true),
            (576, 49, false),
            (512, 42, true),
            (512, 48, false),
        ] {
            let profile = Profile {
                dots,
                columns,
                ..p80.clone()
            };
            assert_eq!(
                profile.validate().is_ok(),
                ok,
                "{dots} dots, {columns} columns"
            );
        }
        let too_wide = Profile { columns: 48, ..p58 };
        assert_eq!(
            too_wide.validate().unwrap_err().field(),
            Some("profile.columns"),
            "48 columns on a 384-dot head"
        );
    }

    #[test]
    fn profiles_cross_the_ipc_in_camel_case() {
        let json = serde_json::to_value(Profile::preset(Preset::Epson, PaperWidth::Mm58)).unwrap();
        assert_eq!(json["width"], "mm58");
        assert_eq!(json["codePage"], "pc858");
        assert_eq!(json["codePageSelector"], 19);
        assert_eq!(json["render"], "auto");
    }
}
