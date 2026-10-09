//! Barcode symbols for label previews and A4 sheets (LBL-004). Label printers draw their own
//! barcodes from the TSPL/ZPL commands; these modules feed the PNG preview and the SVG of the A4
//! sheet, so the screen shows the symbol the printer makes.

use crate::bitmap::Bitmap;
use crate::error::HwError;
use crate::labels::model::{LabelBarcode, LabelSymbology};
use barcoders::sym::code39::Code39;
use barcoders::sym::code128::Code128;
use barcoders::sym::ean8::EAN8;
use barcoders::sym::ean13::EAN13;
use barcoders::sym::tf::TF;
use qrcode::{Color, EcLevel, QrCode};

/// Modules of white on each side of a linear symbol.
pub const QUIET_MODULES: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Symbol {
    /// One entry per module, `true` for a bar; quiet zones excluded.
    Linear(Vec<bool>),
    /// A square of `size × size` modules, row by row.
    Matrix { size: usize, dark: Vec<bool> },
}

fn invalid(reason: impl ToString) -> HwError {
    HwError::Barcode(reason.to_string())
}

fn modules(encoded: Vec<u8>) -> Vec<bool> {
    encoded.into_iter().map(|m| m == 1).collect()
}

/// `barcoders` needs the starting set: C (two digits a symbol) for an even run of digits.
fn code128_text(data: &str) -> String {
    if data.len() >= 4 && data.len().is_multiple_of(2) && data.bytes().all(|b| b.is_ascii_digit()) {
        format!("\u{0106}{data}")
    } else {
        format!("\u{0181}{data}")
    }
}

/// LBL-004: `data` fits Code 39 (capitals, digits, `- . space $ / + %`, 43 at most) or ITF (an
/// even number of digits, 2 to 30) as `LabelBarcode::check` says; the other symbologies pass.
/// Run before anything encodes or prints them (`barcoders` would add a check digit to an odd ITF
/// and print other digits than the label's).
pub(crate) fn check(symbology: LabelSymbology, data: &str) -> Result<(), HwError> {
    LabelBarcode {
        symbology,
        data: data.to_owned(),
    }
    .check()
    .map_err(|_| invalid(format!("{data:?} does not fit {symbology:?}")))
}

/// The modules of `data` in `symbology`: EAN-13 and EAN-8 with their check digit, UPC-A as an
/// EAN-13 with a leading 0, Code 128 in set C for an even run of 4 digits or more (else set B,
/// printable ASCII, 40 characters at most), Code 39 without check character and ITF as checked by
/// [`check`], QR at level M. Anything else is `barcode_invalid`.
pub fn encode(symbology: LabelSymbology, data: &str) -> Result<Symbol, HwError> {
    check(symbology, data)?;
    let digits = |n: usize| data.len() == n && data.bytes().all(|b| b.is_ascii_digit());
    let printable =
        !data.is_empty() && data.len() <= 40 && data.bytes().all(|b| (0x20..0x7F).contains(&b));
    match symbology {
        LabelSymbology::Ean13 if digits(13) => EAN13::new(data)
            .map(|s| Symbol::Linear(modules(s.encode())))
            .map_err(invalid),
        LabelSymbology::UpcA if digits(12) => EAN13::new(format!("0{data}"))
            .map(|s| Symbol::Linear(modules(s.encode())))
            .map_err(invalid),
        LabelSymbology::Ean8 if digits(8) => EAN8::new(data)
            .map(|s| Symbol::Linear(modules(s.encode())))
            .map_err(invalid),
        LabelSymbology::Code128 if printable => Code128::new(code128_text(data))
            .map(|s| Symbol::Linear(modules(s.encode())))
            .map_err(invalid),
        LabelSymbology::Code39 => Code39::new(data)
            .map(|s| Symbol::Linear(modules(s.encode())))
            .map_err(invalid),
        LabelSymbology::Itf => TF::interleaved(data)
            .map(|s| Symbol::Linear(modules(s.encode())))
            .map_err(invalid),
        LabelSymbology::Qr if !data.is_empty() && data.len() <= 300 => {
            let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M)
                .map_err(invalid)?;
            let size = code.width();
            let dark = code
                .to_colors()
                .into_iter()
                .map(|c| c == Color::Dark)
                .collect();
            Ok(Symbol::Matrix { size, dark })
        }
        _ => Err(invalid(format!("{data:?} does not fit {symbology:?}"))),
    }
}

/// Modules across: the bars of a linear symbol, the side of a matrix.
pub fn width_modules(symbol: &Symbol) -> u32 {
    let n = match symbol {
        Symbol::Linear(bars) => bars.len(),
        Symbol::Matrix { size, .. } => *size,
    };
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// `module` dots a module; linear symbols `height` dots high, matrices square.
pub fn draw(symbol: &Symbol, module: u32, height: u32) -> Bitmap {
    let module = module.max(1);
    match symbol {
        Symbol::Linear(bars) => {
            let mut out = Bitmap::new(width_modules(symbol) * module, height);
            for (i, bar) in (0_u32..).zip(bars) {
                if *bar {
                    for x in i * module..(i + 1) * module {
                        for y in 0..height {
                            out.set(x, y, true);
                        }
                    }
                }
            }
            out
        }
        Symbol::Matrix { dark, .. } => {
            let size = width_modules(symbol).max(1);
            let side = size * module;
            let mut out = Bitmap::new(side, side);
            for (i, cell) in (0_u32..).zip(dark) {
                if *cell {
                    let (cx, cy) = (i % size, i / size);
                    for y in cy * module..(cy + 1) * module {
                        for x in cx * module..(cx + 1) * module {
                            out.set(x, y, true);
                        }
                    }
                }
            }
            out
        }
    }
}

/// `"1011…"`: the whole linear symbol, or one string per matrix row (the A4 sheet's SVG).
pub fn rows(symbol: &Symbol) -> Vec<String> {
    let bit = |b: &bool| if *b { '1' } else { '0' };
    match symbol {
        Symbol::Linear(bars) => vec![bars.iter().map(bit).collect()],
        Symbol::Matrix { size, dark } => dark
            .chunks((*size).max(1))
            .map(|row| row.iter().map(bit).collect())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear(symbology: LabelSymbology, data: &str) -> Vec<bool> {
        match encode(symbology, data).unwrap() {
            Symbol::Linear(bars) => bars,
            Symbol::Matrix { .. } => panic!("{data} is linear"),
        }
    }

    #[test]
    fn ean_upc_code128_and_qr_encode_to_modules() {
        let ean = linear(LabelSymbology::Ean13, "6130123456782");
        assert_eq!(ean.len(), 95);
        assert_eq!(&ean[..3], &[true, false, true], "start guard");
        assert_eq!(linear(LabelSymbology::UpcA, "036000291452").len(), 95);
        assert_eq!(linear(LabelSymbology::Ean8, "96385074").len(), 67);
        assert_eq!(
            linear(LabelSymbology::Code128, "20260928").len(),
            79,
            "set C: 4 symbols"
        );
        assert_eq!(
            linear(LabelSymbology::Code128, "P000123").len(),
            112,
            "set B: 7 symbols"
        );
        let Symbol::Matrix { size, dark } =
            encode(LabelSymbology::Qr, "https://dzbuild.com/p/123").unwrap()
        else {
            panic!("a QR code is a matrix");
        };
        assert!(size >= 21);
        assert_eq!(dark.len(), size * size);
    }

    #[test]
    fn data_that_does_not_fit_its_symbology_is_refused() {
        for (symbology, data) in [
            (LabelSymbology::Ean13, "6130123456780"),
            (LabelSymbology::Ean13, "61301234"),
            (LabelSymbology::Ean8, "96385075"),
            (LabelSymbology::UpcA, "03600029145"),
            (LabelSymbology::Code128, ""),
            (LabelSymbology::Code128, "حليب"),
        ] {
            assert_eq!(
                encode(symbology, data).unwrap_err().code(),
                "barcode_invalid",
                "{data}"
            );
        }
    }

    #[test]
    fn code39_and_itf_encode_and_print_natively() {
        use pos_core::labels::build::LabelSymbology;
        let c39 = encode(LabelSymbology::Code39, "REF-12").unwrap();
        let itf = encode(LabelSymbology::Itf, "12345678").unwrap();
        assert!(matches!(c39, Symbol::Linear(ref bars) if !bars.is_empty()));
        assert!(matches!(itf, Symbol::Linear(ref bars) if !bars.is_empty()));
        assert!(encode(LabelSymbology::Itf, "123").is_err());
        let boxed = |s: LabelSymbology, data: &str| crate::labels::layout::BarcodeBox {
            x: 10,
            y: 20,
            width: 200,
            height: 80,
            module: 2,
            symbology: s,
            data: data.into(),
            hri: true,
        };
        assert_eq!(
            crate::labels::tspl::barcode(&boxed(LabelSymbology::Code39, "REF-12"), 10).unwrap(),
            "BARCODE 10,20,\"39\",80,1,0,2,5,\"REF-12\""
        );
        assert_eq!(
            crate::labels::zpl::barcode(&boxed(LabelSymbology::Itf, "12345678"), 10).unwrap(),
            "^FO10,20^BY2^B2N,80,Y,N,N^FD12345678^FS"
        );
        // the other half of each pair
        assert_eq!(
            crate::labels::tspl::barcode(&boxed(LabelSymbology::Itf, "12345678"), 10).unwrap(),
            "BARCODE 10,20,\"25\",80,1,0,2,5,\"12345678\""
        );
        assert_eq!(
            crate::labels::zpl::barcode(&boxed(LabelSymbology::Code39, "REF-12"), 10).unwrap(),
            "^FO10,20^BY2,2.5^B3N,N,80,Y,N^FDREF-12^FS"
        );
        // `/` is a shift character of full-ASCII Code 39: such data goes as standard Code 39
        assert_eq!(
            crate::labels::tspl::barcode(&boxed(LabelSymbology::Code39, "A/12"), 10).unwrap(),
            "BARCODE 10,20,\"39S\",80,1,0,2,5,\"A/12\""
        );
        // narrow:wide ratios the TSPL manual lists for Code 39 and ITF (1:3 at odd widths)
        let mut thin = boxed(LabelSymbology::Itf, "12345678");
        thin.module = 1;
        assert_eq!(
            crate::labels::tspl::barcode(&thin, 10).unwrap(),
            "BARCODE 10,20,\"25\",80,1,0,1,3,\"12345678\""
        );
        // data the symbology cannot carry never reaches the printer
        for (symbology, data) in [
            (LabelSymbology::Code39, "ref-12"),
            (LabelSymbology::Code39, ""),
            (LabelSymbology::Itf, "12345"),
            (LabelSymbology::Itf, "12AB"),
        ] {
            assert_eq!(
                encode(symbology, data).unwrap_err().code(),
                "barcode_invalid",
                "{data}"
            );
            assert_eq!(
                crate::labels::tspl::barcode(&boxed(symbology, data), 10)
                    .unwrap_err()
                    .code(),
                "barcode_invalid"
            );
            assert_eq!(
                crate::labels::zpl::barcode(&boxed(symbology, data), 10)
                    .unwrap_err()
                    .code(),
                "barcode_invalid"
            );
        }
        // the A4 sheet and the preview draw them like Code 128
        assert_eq!(rows(&itf).len(), 1);
        assert!(rows(&c39)[0].starts_with('1') && rows(&c39)[0].ends_with('1'));
    }

    #[test]
    fn modules_become_dots_and_rows() {
        let symbol = Symbol::Linear(vec![true, false, true]);
        let dots = draw(&symbol, 2, 5);
        assert_eq!((dots.width(), dots.height(), dots.black_dots()), (6, 5, 20));
        assert_eq!(rows(&symbol), ["101"]);
        assert_eq!(width_modules(&symbol), 3);
        let square = Symbol::Matrix {
            size: 2,
            dark: vec![true, false, false, true],
        };
        assert_eq!(rows(&square), ["10", "01"]);
        assert_eq!(draw(&square, 3, 0).width(), 6);
    }
}
