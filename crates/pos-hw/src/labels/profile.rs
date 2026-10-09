//! Label printer profiles (HW-013): language, head resolution, darkness, speed, media sensing,
//! offsets and the bitmap polarity, with presets for the models sold in Algeria.

use crate::error::HwError;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LabelLanguage {
    Tspl,
    Zpl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LabelDpi {
    D203,
    D300,
}

impl LabelDpi {
    pub const fn dots_per_mm(self) -> u32 {
        match self {
            LabelDpi::D203 => 8,
            LabelDpi::D300 => 12,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LabelMedia {
    Gap,
    BlackMark,
    Continuous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LabelPreset {
    Xprinter,
    Tsc,
    SmartPos,
    Zebra,
    Godex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelProfile {
    pub preset: LabelPreset,
    pub language: LabelLanguage,
    pub dpi: LabelDpi,
    /// 0–15 (TSPL `DENSITY`; ZPL `~SD` gets twice as much).
    pub darkness: u8,
    /// Inches per second, 1–6.
    pub speed: u8,
    pub media: LabelMedia,
    /// Shifts the whole label to the end and down, in tenths of a millimetre (0–5 mm).
    pub offset_x_dmm: u16,
    pub offset_y_dmm: u16,
    /// TSPL `BITMAP` data: `true` when a 0 bit is a black dot (the TSC manual's convention).
    pub bitmap_black_is_zero: bool,
}

impl LabelProfile {
    pub fn preset(preset: LabelPreset) -> LabelProfile {
        let tspl = LabelProfile {
            preset,
            language: LabelLanguage::Tspl,
            dpi: LabelDpi::D203,
            darkness: 8,
            speed: 4,
            media: LabelMedia::Gap,
            offset_x_dmm: 0,
            offset_y_dmm: 0,
            bitmap_black_is_zero: true,
        };
        match preset {
            LabelPreset::Xprinter | LabelPreset::Tsc | LabelPreset::SmartPos => tspl,
            LabelPreset::Zebra | LabelPreset::Godex => LabelProfile {
                language: LabelLanguage::Zpl,
                bitmap_black_is_zero: false,
                ..tspl
            },
        }
    }

    pub fn validate(&self) -> Result<(), HwError> {
        if self.darkness > 15 {
            return Err(HwError::invalid("profile.darkness"));
        }
        if !(1..=6).contains(&self.speed) {
            return Err(HwError::invalid("profile.speed"));
        }
        if self.offset_x_dmm > 50 {
            return Err(HwError::invalid("profile.offsetXDmm"));
        }
        if self.offset_y_dmm > 50 {
            return Err(HwError::invalid("profile.offsetYDmm"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_pick_the_language_and_validate() {
        for (preset, language) in [
            (LabelPreset::Xprinter, LabelLanguage::Tspl),
            (LabelPreset::Tsc, LabelLanguage::Tspl),
            (LabelPreset::SmartPos, LabelLanguage::Tspl),
            (LabelPreset::Zebra, LabelLanguage::Zpl),
            (LabelPreset::Godex, LabelLanguage::Zpl),
        ] {
            let p = LabelProfile::preset(preset);
            assert_eq!(p.language, language);
            p.validate().unwrap();
        }
        let base = LabelProfile::preset(LabelPreset::Xprinter);
        let field = |p: LabelProfile| p.validate().unwrap_err().field().map(str::to_owned);
        assert_eq!(
            field(LabelProfile {
                darkness: 16,
                ..base.clone()
            })
            .as_deref(),
            Some("profile.darkness")
        );
        assert_eq!(
            field(LabelProfile {
                speed: 0,
                ..base.clone()
            })
            .as_deref(),
            Some("profile.speed")
        );
        assert_eq!(
            field(LabelProfile {
                offset_x_dmm: 51,
                ..base.clone()
            })
            .as_deref(),
            Some("profile.offsetXDmm")
        );
        assert_eq!(LabelDpi::D300.dots_per_mm(), 12);
        let json = serde_json::to_value(&base).unwrap();
        assert_eq!(
            (json["language"].as_str(), json["dpi"].as_str()),
            (Some("tspl"), Some("d203"))
        );
    }
}
