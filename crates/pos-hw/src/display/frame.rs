//! What the customer sees, as the till sends it: amounts come from the backend (`sales_preview`),
//! the names in the document language, the Latin name for the pole display.

use crate::error::HwError;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_docs::a4::qr::QrMatrix;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode {
    Idle,
    Sale,
    Paid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DisplayLine {
    /// In the ticket's language (UX-041: the display follows the document language).
    pub name: String,
    /// French or English name for the pole display; empty when the product has none.
    pub latin_name: String,
    pub qty: Qty,
    pub unit_code: String,
    pub unit_price: Money,
    pub total: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DisplayWeight {
    pub net: Qty,
    pub stable: bool,
    pub unit_price: Option<Money>,
    pub tare: Qty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DisplayFrame {
    pub mode: DisplayMode,
    pub lang: Lang,
    /// The line just added or selected.
    pub line: Option<DisplayLine>,
    pub items: Qty,
    /// Amount due.
    pub total: Money,
    /// PRM-006 "you saved".
    pub saved: Money,
    pub paid: Option<Money>,
    pub change: Option<Money>,
    /// VRT-010: the weight on the scale with its price per kg.
    pub weight: Option<DisplayWeight>,
    /// HW-042: the shop's BaridiMob QR (`payments_baridpay_qr`), shown while the payment dialog
    /// has BaridiMob selected.
    #[serde(default)]
    #[specta(optional)]
    pub pay_qr: Option<QrMatrix>,
}

/// The largest QR (version 40).
const QR_MAX_SIZE: u32 = 177;

impl DisplayFrame {
    pub fn idle(lang: Lang) -> DisplayFrame {
        DisplayFrame {
            mode: DisplayMode::Idle,
            lang,
            line: None,
            items: Qty::ZERO,
            total: Money::ZERO,
            saved: Money::ZERO,
            paid: None,
            change: None,
            weight: None,
            pay_qr: None,
        }
    }

    pub fn validate(&self) -> Result<(), HwError> {
        let long = |s: &str| s.chars().count() > 120;
        if let Some(line) = &self.line {
            if long(&line.name) || long(&line.latin_name) || long(&line.unit_code) {
                return Err(HwError::setting("frame.line"));
            }
            if line.total.is_negative() || line.unit_price.is_negative() {
                return Err(HwError::setting("frame.line"));
            }
        }
        let negative = [Some(self.total), Some(self.saved), self.paid, self.change]
            .into_iter()
            .flatten()
            .any(Money::is_negative);
        if negative {
            return Err(HwError::setting("frame.total"));
        }
        if let Some(qr) = &self.pay_qr {
            let square = u64::from(qr.size) * u64::from(qr.size);
            let fits = (1..=QR_MAX_SIZE).contains(&qr.size)
                && u64::try_from(qr.modules.len()).is_ok_and(|n| n == square)
                && qr.modules.bytes().all(|b| b == b'0' || b == b'1');
            if !fits {
                return Err(HwError::setting("frame.payQr"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_cross_the_ipc_in_camel_case_and_are_validated() {
        let line = DisplayLine {
            name: "حليب كانديا".into(),
            latin_name: "Lait Candia".into(),
            qty: Qty(1_000),
            unit_code: "pc".into(),
            unit_price: Money(15_000),
            total: Money(15_000),
        };
        let frame = DisplayFrame {
            mode: DisplayMode::Sale,
            line: Some(line.clone()),
            items: Qty(1_000),
            total: Money(15_000),
            ..DisplayFrame::idle(Lang::Ar)
        };
        let json = serde_json::to_value(&frame).unwrap();
        assert_eq!(json["mode"], "sale");
        assert_eq!(json["lang"], "ar");
        assert_eq!(json["line"]["latinName"], "Lait Candia");
        assert_eq!(json["line"]["unitPrice"], 15_000);
        assert!(json["paid"].is_null() && json["weight"].is_null());
        assert_eq!(serde_json::from_value::<DisplayFrame>(json).unwrap(), frame);
        frame.validate().unwrap();
        let field = |f: DisplayFrame| f.validate().unwrap_err().field().map(str::to_owned);
        let long = DisplayLine {
            latin_name: "x".repeat(121),
            ..line.clone()
        };
        assert_eq!(
            field(DisplayFrame {
                line: Some(long),
                ..frame.clone()
            })
            .as_deref(),
            Some("frame.line")
        );
        let negative = DisplayLine {
            total: Money(-1),
            ..line
        };
        assert_eq!(
            field(DisplayFrame {
                line: Some(negative),
                ..frame.clone()
            })
            .as_deref(),
            Some("frame.line")
        );
        assert_eq!(
            field(DisplayFrame {
                change: Some(Money(-1)),
                ..frame
            })
            .as_deref(),
            Some("frame.total")
        );
    }

    /// HW-042: the BaridiMob QR rides the frame while the customer pays by BaridiMob; a till
    /// that sends no `payQr` (M4's frames) still parses, and a matrix that is not square is
    /// refused.
    #[test]
    fn the_baridimob_qr_rides_the_frame() {
        let qr = pos_docs::a4::qr::qr("BARIDIMOB:00799999000123456789").unwrap();
        let frame = DisplayFrame {
            mode: DisplayMode::Sale,
            total: Money(15_000),
            pay_qr: Some(qr.clone()),
            ..DisplayFrame::idle(Lang::Fr)
        };
        let json = serde_json::to_value(&frame).unwrap();
        assert_eq!(json["payQr"]["size"], qr.size);
        assert_eq!(serde_json::from_value::<DisplayFrame>(json).unwrap(), frame);
        frame.validate().unwrap();
        let mut old = serde_json::to_value(DisplayFrame::idle(Lang::Fr)).unwrap();
        old.as_object_mut().unwrap().remove("payQr");
        assert_eq!(
            serde_json::from_value::<DisplayFrame>(old).unwrap().pay_qr,
            None
        );
        let torn = pos_docs::a4::qr::QrMatrix {
            size: qr.size,
            modules: "1".repeat(3),
        };
        let refused = DisplayFrame {
            pay_qr: Some(torn),
            ..frame
        }
        .validate()
        .unwrap_err();
        assert_eq!(refused.field(), Some("frame.payQr"));
    }
}
